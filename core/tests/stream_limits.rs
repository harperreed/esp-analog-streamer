// ABOUTME: Exercises firmware history profiles with the real encoded ring and encoder.
// ABOUTME: Guards delayed readers, bounded memory and independent socket deadlines.

#[path = "../../firmware/src/stream_limits.rs"]
mod stream_limits;

use esp_shazam_core::{
    flac,
    flac_ring::{FlacRing, Overrun},
};
use stream_limits::{flac_history_frames, flac_write_limit_ms};

#[test]
fn eleven_seconds_of_delayed_audio_remain_exact_and_ordered() {
    let mut ring = FlacRing::try_new(flac_history_frames(false), flac::MAX_FRAME_BYTES).unwrap();
    let mut fast = ring.cursor_now();
    let mut slow = ring.cursor_now();
    let mut encoder = flac::Encoder::new();
    let mut encoded = Vec::new();
    let mut output = vec![0; flac::MAX_FRAME_BYTES];
    // 129 blocks cover 11.008 seconds at 48 kHz. Repeat across ring wraparound.
    for round in 0..2 {
        let mut expected = Vec::new();
        for index in 0..129 {
            encoder.reset();
            encoder.push(&vec![
                ((round * 129 + index) as i32) << 8;
                2 * flac::BLOCK_FRAMES
            ]);
            encoder.encode_block(round * 129 + index, &mut encoded);
            ring.push(&encoded).unwrap();
            assert_eq!(ring.read(&mut fast, &mut output), Ok(encoded.len()));
            assert_eq!(&output[..encoded.len()], encoded);
            expected.push(encoded.clone());
        }
        for frame in expected {
            assert_eq!(ring.read(&mut slow, &mut output), Ok(frame.len()));
            assert_eq!(&output[..frame.len()], frame);
        }
        assert_eq!(ring.read(&mut slow, &mut output), Ok(0));
    }
}

#[test]
fn profiles_retain_exact_capacity_then_reject_overwritten_audio() {
    for (enabled, expected_capacity) in [(false, 141), (true, 24)] {
        let mut ring =
            FlacRing::try_new(flac_history_frames(enabled), flac::MAX_FRAME_BYTES).unwrap();
        let mut reader = ring.cursor_now();
        let mut stale = ring.cursor_now();
        for index in 0..expected_capacity {
            ring.push(&[index as u8]).unwrap();
        }
        let mut output = [0];
        assert_eq!(ring.read(&mut reader, &mut output), Ok(1));
        assert_eq!(output, [0]);
        ring.push(&[255]).unwrap();
        let before = stale;
        assert_eq!(ring.read(&mut stale, &mut output), Err(Overrun));
        assert_eq!(stale, before);
    }
}

#[test]
fn history_fits_the_previous_payload_budget_without_extending_write_deadlines() {
    // Original 94-slot 24-bit payload: 94 * 24,591 = 2,311,554 bytes.
    let off = flac_history_frames(false);
    assert_eq!(off * flac::MAX_FRAME_BYTES, 2_312_259);
    assert_eq!(off * flac::MAX_FRAME_BYTES - 2_311_554, 705);
    assert_eq!(off * 4 - 94 * 4, 188); // ESP32 usize length metadata.
    assert_eq!(1_000 * off * flac::BLOCK_FRAMES / 48_000, 12_032);
    assert_eq!(flac_history_frames(true) * flac::MAX_FRAME_BYTES, 393_576);
    assert_eq!(flac_write_limit_ms(false), 8_021);
    assert_eq!(flac_write_limit_ms(true), 2_048);
}

#[test]
fn mp3_history_and_writes_have_separate_fixed_bounds() {
    assert_eq!(stream_limits::mp3_history_frames(), 500);
    assert_eq!(500 * 960, 480_000);
    assert_eq!(500 * 1152, 12 * 48_000);
    assert_eq!(stream_limits::mp3_write_limit_ms(), 8_016);
}
