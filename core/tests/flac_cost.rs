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
