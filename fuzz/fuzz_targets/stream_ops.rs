#![no_main]
// ABOUTME: Drives legal socket outcomes and sequential raw-ring publication events.
// ABOUTME: Bounds virtual time and operations without threads, sockets or sleeps.
use esp_shazam_core::{
    capture_buffer::CaptureBuffer,
    stream_write::{Action, WriteProgress},
};
use esp_shazam_fuzz::{checked_polled, checked_sent};
use libfuzzer_sys::fuzz_target;
use std::{io, time::Duration};
fuzz_target!(|input: &[u8]| {
    let data = &input[..input.len().min(2048)];
    if data.len() < 4 {
        return;
    }
    let length = usize::from(data[0]);
    let deadline = Duration::from_millis(u64::from(data[1]));
    let mut progress = Some(WriteProgress::new(length, deadline));
    let mut now = Duration::ZERO;
    let (mut producer, ring) = CaptureBuffer::new((usize::from(data[2]) % 4 + 1) * 2, 2);
    let mut cursor = ring.cursor_now().unwrap();
    let mut scratch = [0; 4];
    let mut encoded =
        esp_shazam_core::flac_ring::FlacRing::try_new(usize::from(data[3]) % 4 + 1, 16).unwrap();
    let mut encoded_cursor = encoded.cursor_now();
    let mut bytes = [0; 16];
    for op in data[4..].chunks(4).take(128) {
        now = now.saturating_add(Duration::from_millis(u64::from(op[0] % 8)));
        match op[0] % 4 {
            0 => {
                producer
                    .publish(&[i32::from(op[0]); 4], now.as_micros() as u64, || {
                        now.as_micros() as u64
                    })
                    .unwrap();
            }
            1 => {
                producer.invalidate().unwrap();
            }
            2 => {
                let before = cursor;
                match ring.read(&mut cursor, &mut scratch).map(|read| read.frames) {
                    Ok(n) => assert!(n <= 2),
                    Err(_) => assert_eq!(before, cursor),
                }
            }
            _ => {
                cursor = ring.cursor_now().unwrap();
            }
        }
        match op.get(3).copied().unwrap_or(0) % 4 {
            0 => {
                encoded
                    .push(
                        op,
                        esp_shazam_core::flac_ring::SourceMetadata {
                            raw_generation: 0,
                            source_start: 0,
                            source_end: 1,
                            oldest_accepted_us: now.as_micros() as u64,
                            raw_publication_us: now.as_micros() as u64,
                        },
                        || now.as_micros() as u64,
                    )
                    .unwrap();
            }
            1 => {
                encoded.reset().unwrap();
            }
            2 => {
                let before = encoded_cursor;
                match encoded.read(&mut encoded_cursor, &mut bytes) {
                    Ok(read) => {
                        assert!(read.bytes <= bytes.len());
                        if let Some(receipt) = read.receipt {
                            assert!(
                                receipt.metadata.encoded_production_us <= now.as_micros() as u64
                            );
                        }
                    }
                    Err(_) => assert_eq!(before, encoded_cursor),
                }
            }
            _ => {
                if let Some(cursor) = encoded.cursor_retained(1).unwrap() {
                    encoded_cursor = cursor;
                }
            }
        }
        let Some(write) = progress.as_mut() else {
            continue;
        };
        let accepted = write.accepted();
        assert!(accepted <= length);
        let result = match write.next(now) {
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::TimedOut);
                assert!(now >= deadline);
                progress = None;
                continue;
            }
            Ok(Action::Complete) => {
                assert_eq!(accepted, length);
                progress = None;
                continue;
            }
            Ok(Action::Send { offset }) => {
                assert_eq!(offset, accepted);
                match op.get(1).copied().unwrap_or(0) % 5 {
                    0 => checked_sent(
                        write,
                        Ok(usize::from(op.get(2).copied().unwrap_or(0)) % (length - accepted + 1)),
                        now,
                    ),
                    1 => checked_sent(write, Err(io::ErrorKind::WouldBlock.into()), now),
                    2 => checked_sent(write, Err(io::ErrorKind::Interrupted.into()), now),
                    3 => checked_sent(write, Err(io::ErrorKind::BrokenPipe.into()), now),
                    _ => checked_sent(write, Ok(length - accepted), now),
                }
            }
            Ok(Action::Poll { timeout_ms }) => {
                assert!(timeout_ms > 0 && now < deadline);
                match op.get(1).copied().unwrap_or(0) % 4 {
                    0 => checked_polled(write, Ok(false), now),
                    1 => checked_polled(write, Ok(true), now),
                    2 => checked_polled(write, Err(io::ErrorKind::Interrupted.into()), now),
                    _ => checked_polled(write, Err(io::ErrorKind::BrokenPipe.into()), now),
                }
            }
        };
        assert!(write.accepted() >= accepted && write.accepted() <= length);
        if result.is_err() {
            progress = None;
        }
    }
});
