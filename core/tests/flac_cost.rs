// ABOUTME: Runs the device benchmark's frame assertions against actual production encoder output.
// ABOUTME: Covers zero/DC blocks and reusable output storage without needing ESP-IDF or hardware.

#[path = "../../firmware/examples/flac-cost/contract.rs"]
mod contract;

use esp_shazam_core::flac::{Encoder, BLOCK_FRAMES, CHANNELS, MAX_FRAME_BYTES};

fn constant_frames(kind: &str, channels: [i32; CHANNELS]) {
    let mut encoder = Encoder::new();
    let samples: Vec<i32> = channels
        .into_iter()
        .cycle()
        .take(BLOCK_FRAMES * CHANNELS)
        .collect();
    let mut frame = Vec::with_capacity(MAX_FRAME_BYTES);
    let initial_ptr = frame.as_ptr();
    let initial_capacity = frame.capacity();
    for _ in 0..2 {
        encoder.reset();
        for chunk in samples.chunks(240 * CHANNELS) {
            encoder.push(chunk);
        }
        encoder.encode_block(0, &mut frame);
        assert_eq!(
            frame.len(),
            14,
            "six-byte header, two three-byte channels, CRC16"
        );
        contract::assert_frame(kind, &frame, initial_ptr, initial_capacity);
    }
}

#[test]
fn benchmark_accepts_zero_frames_from_the_current_encoder() {
    constant_frames("zero", [0, 0]);
}

#[test]
fn benchmark_accepts_dc_frames_from_the_current_encoder() {
    constant_frames("dc", [123_456, -123_456]);
}

#[test]
fn benchmark_stereo_cases_are_deterministic_and_cover_channel_relationships() {
    let mut samples = vec![0; BLOCK_FRAMES * CHANNELS];
    let mut repeated = samples.clone();
    for kind in contract::KINDS {
        contract::fill_input(kind, &mut samples);
        contract::fill_input(kind, &mut repeated);
        assert_eq!(samples, repeated, "{kind}");
        if !matches!(kind, "zero" | "dc") {
            assert!(
                samples.chunks_exact(2).any(|p| p != &samples[..2]),
                "{kind} must vary"
            );
        }
        match kind {
            "identical" => assert!(samples.chunks_exact(2).all(|p| p[0] == p[1])),
            "opposite" => assert!(samples.chunks_exact(2).all(|p| p[0] == -p[1])),
            "hard_left" => assert!(samples.chunks_exact(2).all(|p| p[1] == 0)),
            "hard_right" => assert!(samples.chunks_exact(2).all(|p| p[0] == 0)),
            "extremes" => assert!(samples.iter().all(|s| matches!(*s, -8_388_608 | 8_388_352))),
            "correlated" => assert!(samples.chunks_exact(2).all(|p| (p[0] - p[1]).abs() <= 4096)),
            _ => (),
        }
        let mut encoder = Encoder::new();
        let mut frame = Vec::with_capacity(MAX_FRAME_BYTES);
        let pointer = frame.as_ptr();
        let capacity = frame.capacity();
        encoder.push(&samples);
        encoder.encode_block(0, &mut frame);
        contract::assert_frame(kind, &frame, pointer, capacity);
        assert_eq!(MAX_FRAME_BYTES, 16_399);
    }
}
