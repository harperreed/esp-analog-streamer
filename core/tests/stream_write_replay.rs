// ABOUTME: Replays the production frame-write decisions with virtual time and real encoded PCM.
// ABOUTME: Checks exact delivery, timeout edges and socket-result handling without sockets or sleeps.

use std::io;
use std::time::Duration;

use esp_shazam_core::capture_buffer::CaptureBuffer;
use esp_shazam_core::flac::{self, BLOCK_FRAMES, MAX_FRAME_BYTES};
use esp_shazam_core::flac_ring::FlacRing;
use esp_shazam_core::stream_write::{Action, WriteProgress};

const DEADLINE: Duration = Duration::from_micros(100);

#[test]
fn empty_writes_complete_even_at_an_expired_deadline() {
    let state = WriteProgress::new(0, DEADLINE);
    assert_eq!(state.next(DEADLINE).unwrap(), Action::Complete);
}

#[test]
fn no_send_or_poll_starts_at_the_deadline() {
    let state = WriteProgress::new(3, DEADLINE);
    assert_eq!(
        state.next(DEADLINE).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    let mut state = WriteProgress::new(3, DEADLINE);
    state.sent(Err(io::ErrorKind::WouldBlock.into())).unwrap();
    assert_eq!(
        state.next(DEADLINE).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn a_final_send_can_finish_late_but_partial_progress_cannot_extend_the_budget() {
    let mut complete = WriteProgress::new(3, DEADLINE);
    assert_eq!(
        complete.next(Duration::ZERO).unwrap(),
        Action::Send { offset: 0 }
    );
    complete.sent(Ok(3)).unwrap();
    assert_eq!(
        complete.next(DEADLINE + DEADLINE).unwrap(),
        Action::Complete
    );

    let mut partial = WriteProgress::new(3, DEADLINE);
    partial.sent(Ok(2)).unwrap();
    assert_eq!(
        partial.next(DEADLINE).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(partial.accepted(), 2);
}

#[test]
fn readiness_and_interruption_retry_the_same_unsent_suffix() {
    let mut state = WriteProgress::new(9, DEADLINE);
    state.sent(Ok(2)).unwrap();
    state.sent(Err(io::ErrorKind::Interrupted.into())).unwrap();
    assert_eq!(
        state.next(Duration::ZERO).unwrap(),
        Action::Send { offset: 2 }
    );
    state.sent(Err(io::ErrorKind::WouldBlock.into())).unwrap();
    assert_eq!(
        state.next(Duration::ZERO).unwrap(),
        Action::Poll { timeout_ms: 1 }
    );
    state.polled(Ok(false)).unwrap();
    state
        .polled(Err(io::ErrorKind::Interrupted.into()))
        .unwrap();
    assert_eq!(
        state.next(Duration::ZERO).unwrap(),
        Action::Poll { timeout_ms: 1 }
    );
    // Any positive poll result, including HUP/ERR/NVAL, retries send for its errno.
    state.polled(Ok(true)).unwrap();
    assert_eq!(
        state.next(Duration::ZERO).unwrap(),
        Action::Send { offset: 2 }
    );
    state.sent(Err(io::ErrorKind::WouldBlock.into())).unwrap();
    assert_eq!(
        state.next(Duration::ZERO).unwrap(),
        Action::Poll { timeout_ms: 1 }
    );
}

#[test]
fn poll_timeouts_round_up_and_saturate() {
    let mut state = WriteProgress::new(1, Duration::from_nanos(1_000_001));
    state.sent(Err(io::ErrorKind::WouldBlock.into())).unwrap();
    assert_eq!(
        state.next(Duration::ZERO).unwrap(),
        Action::Poll { timeout_ms: 2 }
    );
    let mut state = WriteProgress::new(1, Duration::MAX);
    state.sent(Err(io::ErrorKind::WouldBlock.into())).unwrap();
    assert_eq!(
        state.next(Duration::ZERO).unwrap(),
        Action::Poll {
            timeout_ms: i32::MAX
        }
    );
}

#[test]
fn zero_send_and_fatal_send_or_poll_preserve_the_terminal_error() {
    let mut state = WriteProgress::new(1, DEADLINE);
    assert_eq!(
        state.sent(Ok(0)).unwrap_err().kind(),
        io::ErrorKind::WriteZero
    );
    let mut state = WriteProgress::new(1, DEADLINE);
    assert_eq!(
        state
            .sent(Err(io::Error::from_raw_os_error(libc::EPIPE)))
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EPIPE)
    );
    let mut state = WriteProgress::new(1, DEADLINE);
    state.sent(Err(io::ErrorKind::WouldBlock.into())).unwrap();
    assert_eq!(
        state
            .polled(Err(io::Error::from_raw_os_error(libc::EBADF)))
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EBADF)
    );
}

#[derive(Debug, PartialEq, Eq)]
struct Replay {
    bytes: Vec<u8>,
    events: Vec<(u64, Action)>,
}

fn write_virtual(bytes: &[u8], seed: &mut u64, replay: &mut Replay) {
    let mut state = WriteProgress::new(bytes.len(), Duration::from_secs(1));
    let mut tick = 0;
    loop {
        let action = state.next(Duration::from_micros(tick)).unwrap();
        replay.events.push((tick, action));
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        match action {
            Action::Complete => break,
            Action::Send { offset } => match *seed % 7 {
                0 => state.sent(Err(io::ErrorKind::WouldBlock.into())).unwrap(),
                1 => state.sent(Err(io::ErrorKind::Interrupted.into())).unwrap(),
                _ => {
                    let n = (1 + (*seed >> 8) as usize % 97).min(bytes.len() - offset);
                    replay.bytes.extend_from_slice(&bytes[offset..offset + n]);
                    state.sent(Ok(n)).unwrap();
                }
            },
            Action::Poll { .. } => match *seed % 3 {
                0 => state
                    .polled(Err(io::ErrorKind::Interrupted.into()))
                    .unwrap(),
                n => state.polled(Ok(n == 2)).unwrap(),
            },
        }
        tick += 1;
        assert!(tick < 100_000, "finite replay event budget");
    }
    assert_eq!(state.accepted(), bytes.len());
}

fn replay_pcm(mut seed: u64) -> Replay {
    let (mut publisher, raw) = CaptureBuffer::new(2 * BLOCK_FRAMES, 256);
    let mut raw_cursor = raw.cursor_now().unwrap();
    let mut encoded = FlacRing::try_new(2, MAX_FRAME_BYTES).unwrap();
    let mut listener = encoded.cursor_now();
    let mut encoder = flac::Encoder::new();
    let mut frame = Vec::new();
    let mut chunk = [0; 512];
    for first in (0..2 * BLOCK_FRAMES).step_by(256) {
        for (i, samples) in chunk.chunks_exact_mut(2).enumerate() {
            let index = (first + i) as i32;
            samples[0] = -(index % 32768) * 256 + 127;
            samples[1] = ((index * 7) % 32768) * 256 + 63;
        }
        publisher.publish(&chunk, 0, || 0).unwrap();
        assert_eq!(
            raw.read(&mut raw_cursor, &mut chunk)
                .map(|read| read.frames),
            Ok(256)
        );
        encoder.push(&chunk);
        if encoder.frames_wanted() == 0 {
            encoder.encode_block(127 + (first / BLOCK_FRAMES) as u32, &mut frame);
            encoded
                .push(
                    &frame,
                    esp_shazam_core::flac_ring::SourceMetadata {
                        raw_generation: 0,
                        source_start: 0,
                        source_end: 1,
                        oldest_accepted_us: 0,
                        raw_publication_us: 0,
                    },
                    || 0,
                )
                .unwrap();
        }
    }
    let mut replay = Replay {
        bytes: Vec::new(),
        events: Vec::new(),
    };
    write_virtual(&flac::stream_header(), &mut seed, &mut replay);
    let mut bytes = vec![0; MAX_FRAME_BYTES];
    for number in 0..2 {
        let n = encoded
            .read(&mut listener, &mut bytes)
            .map(|read| read.bytes)
            .unwrap();
        assert!(n > 0);
        let n = flac::renumber_frame(&mut bytes, n, number).unwrap();
        write_virtual(&bytes[..n], &mut seed, &mut replay);
    }
    assert_eq!(
        encoded
            .read(&mut listener, &mut bytes)
            .map(|read| read.bytes),
        Ok(0)
    );
    assert!(replay.bytes.len() <= flac::STREAM_HEADER_LEN + 2 * MAX_FRAME_BYTES);
    replay
}

#[test]
fn seeded_partial_writes_decode_actual_capture_and_flac_in_order() {
    let first = replay_pcm(20261010);
    let second = replay_pcm(20261010);
    assert_eq!(
        first, second,
        "same seed produces identical decisions and bytes"
    );
    let another_schedule = replay_pcm(20261011);
    assert_eq!(first.bytes, another_schedule.bytes);
    assert_ne!(first.events, another_schedule.events);
    let mut reader = claxon::FlacReader::new(first.bytes.as_slice()).unwrap();
    let samples = reader.samples().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(samples.len(), 4 * BLOCK_FRAMES);
    for (index, stereo) in samples.chunks_exact(2).enumerate() {
        assert_eq!(
            stereo,
            &[-(index as i32 % 32768), (index as i32 * 7) % 32768]
        );
    }
}
