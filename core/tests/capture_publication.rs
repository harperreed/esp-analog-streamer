// ABOUTME: Verifies shared capture against reference history and real codec/socket consumers.
// ABOUTME: Checks partial raw blocks across fixed FLAC frame boundaries without fake transport.
#![cfg(not(feature = "capture-loom"))]
use esp_shazam_core::{capture_buffer::CaptureBuffer, stream_ring::ReadError};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, rng_seed: proptest::test_runner::RngSeed::Fixed(0xCA970810), ..ProptestConfig::default() })]
    #[test]
    fn randomized_capture_matches_reference(operations in prop::collection::vec((0u8..5, 0usize..12),1..150)) {
        let (mut publisher, ring) = CaptureBuffer::new(12,3);
        let mut cursors = [ring.cursor_now().unwrap();2];
        let mut positions = [0usize;2];
        let mut generations = [0usize;2];
        let mut generation = 0usize;
        let mut history: Vec<i32> = Vec::new();
        let mut pending = Vec::new();
        let mut next = 1i32;
        for (operation,count) in operations {
            match operation {
                0 | 1 => {
                    let samples: Vec<_> = (0..count).flat_map(|_| { let i=next; next+=1; [i,-i]}).collect();
                    publisher.publish(&samples, 0, || 0).unwrap();
                    pending.extend(samples);
                    let complete = pending.len()/6*6;
                    history.extend(pending.drain(..complete));
                    prop_assert_eq!(publisher.staged_frames(),pending.len()/2);
                }
                2 | 3 => {
                    let reader = (operation-2) as usize;
                    let old = cursors[reader];
                    let mut out = vec![0;count*2+1];
                    let result = ring.read(&mut cursors[reader],&mut out).map(|read| read.frames);
                    if generations[reader]!=generation || history.len()/2-positions[reader]>12 {
                        prop_assert_eq!(result,Err(ReadError::Overrun));
                        prop_assert_eq!(cursors[reader],old);
                        cursors[reader]=ring.cursor_now().unwrap();
                        positions[reader]=history.len()/2;
                        generations[reader]=generation;
                    } else {
                        let n = count.min(3-positions[reader]%3).min(history.len()/2-positions[reader]);
                        prop_assert_eq!(result,Ok(n));
                        prop_assert_eq!(&out[..2*n],&history[2*positions[reader]..2*(positions[reader]+n)]);
                        positions[reader]+=n;
                    }
                }
                _ => {
                    prop_assert_eq!(publisher.invalidate(),Ok(pending.len()/2));
                    pending.clear();history.clear();generation+=1;
                }
            }
        }
    }
}

#[test]
fn wav_and_flac_reconstruct_stereo_across_4096_frame_boundary() {
    use esp_shazam_core::{flac, wav};
    let frames = 8400;
    let samples: Vec<_> = (0..frames)
        .flat_map(|i| [i * 256, -(i + 31) * 256])
        .collect();
    let (mut publisher, ring) = CaptureBuffer::new(frames as usize, 240);
    let mut wav_cursor = ring.cursor_now().unwrap();
    let mut flac_cursor = wav_cursor;
    publisher.publish(&samples, 0, || 0).unwrap();
    let mut bytes = Vec::new();
    let mut scratch = [0; 480];
    loop {
        let n = ring
            .read(&mut wav_cursor, &mut scratch)
            .map(|read| read.frames)
            .unwrap();
        if n == 0 {
            break;
        }
        let mut packed = Vec::new();
        wav::pack_s24le(&scratch[..n * 2], &mut packed);
        bytes.extend(packed);
    }
    let reconstructed: Vec<_> = bytes
        .chunks_exact(3)
        .map(|b| i32::from_le_bytes([b[0], b[1], b[2], if b[2] & 128 != 0 { 255 } else { 0 }]))
        .collect();
    assert_eq!(reconstructed, samples);
    let mut encoder = flac::Encoder::new();
    let mut stream = flac::stream_header().to_vec();
    for block in 0..2 {
        let mut filled = 0;
        while filled < flac::BLOCK_FRAMES {
            let wanted = (flac::BLOCK_FRAMES - filled).min(240);
            let n = ring
                .read(&mut flac_cursor, &mut scratch[..wanted * 2])
                .map(|read| read.frames)
                .unwrap();
            assert!(n > 0);
            encoder.push(&scratch[..n * 2]);
            filled += n;
        }
        let mut frame = Vec::new();
        encoder.encode_block(block, &mut frame);
        stream.extend(frame);
    }
    // The encoder's last partial raw block remains available after two complete FLAC blocks.
    assert_eq!(
        ring.read(&mut flac_cursor, &mut scratch)
            .map(|read| read.frames),
        Ok(208)
    );
    assert_eq!(&scratch[..416], &samples[8192 * 2..]);
    let mut decoder = claxon::FlacReader::new(stream.as_slice()).unwrap();
    let decoded: Vec<_> = decoder.samples().map(Result::unwrap).collect();
    assert_eq!(decoded.len(), 8192 * 2);
    assert_eq!(
        decoded,
        samples[..8192 * 2]
            .iter()
            .map(|s| s >> 8)
            .collect::<Vec<_>>()
    );
}

#[test]
fn real_socket_pressure_does_not_hold_capture_publication() {
    use esp_shazam_core::{stream_http::DeadlineWriter, wav};
    use std::{
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (server, _) = listener.accept().unwrap();
    socket2::SockRef::from(&server)
        .set_send_buffer_size(4096)
        .unwrap();
    socket2::SockRef::from(&client)
        .set_recv_buffer_size(4096)
        .unwrap();
    let (mut publisher, ring) = CaptureBuffer::new(96000, 240);
    let mut cursor = ring.cursor_now().unwrap();
    publisher.publish(&[5; 480], 0, || 0).unwrap();
    let (started, ready) = mpsc::channel();
    let writer = thread::spawn(move || {
        let mut out = [0; 480];
        assert_eq!(
            ring.read(&mut cursor, &mut out).map(|read| read.frames),
            Ok(240)
        );
        let mut packed = Vec::new();
        wav::pack_s24le(&out, &mut packed);
        let pressure = packed.repeat(8192);
        started.send(()).unwrap();
        DeadlineWriter::new(&server)
            .write_all_before(&pressure, Instant::now() + Duration::from_millis(200))
    });
    ready.recv().unwrap();
    for _ in 0..450 {
        publisher.publish(&[6; 480], 0, || 0).unwrap();
    }
    assert_eq!(publisher.staged_frames(), 0);
    assert!(writer.join().unwrap().is_err());
    drop(client);
}
