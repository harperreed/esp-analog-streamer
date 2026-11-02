// ABOUTME: Checks frame-size and reusable-output contracts shared by the FLAC cost benchmark and host tests.
// ABOUTME: Keeps the benchmark's zero/DC assertions tied to frames produced by the current encoder.

use esp_shazam_core::flac::{CHANNELS, MAX_FRAME_BYTES};

pub fn assert_frame(kind: &str, frame: &Vec<u8>, initial_ptr: *const u8, initial_capacity: usize) {
    assert_eq!(
        MAX_FRAME_BYTES, 16_399,
        "independent verbatim allocation bound"
    );
    assert!(frame.len() <= MAX_FRAME_BYTES);
    assert!(
        matches!(frame[3] >> 4, 1 | 8),
        "supported stereo assignment"
    );
    if kind == "zero" || kind == "dc" {
        assert_eq!(frame.len(), 14, "constant block at frame number zero");
    }
    assert_eq!(frame.as_ptr(), initial_ptr, "output storage must be reused");
    assert_eq!(frame.capacity(), initial_capacity);
}

pub const KINDS: [&str; 10] = [
    "zero",
    "dc",
    "mixed",
    "noise",
    "identical",
    "opposite",
    "hard_left",
    "hard_right",
    "extremes",
    "correlated",
];

/// Repeated blocks deliberately keep the same PCM and frame number across both images.
pub fn fill_input(kind: &str, input: &mut [i32]) {
    let mut state = 0x1234_5678u32;
    for (index, pair) in input.chunks_exact_mut(CHANNELS).enumerate() {
        let mut noise = [0; CHANNELS];
        for sample in &mut noise {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *sample = (state as i32) >> 8;
        }
        // A bounded varying waveform leaves room for the correlated channel's residual.
        let varying = (((index * 997) % 32768) as i32 - 16384) * 256;
        pair.copy_from_slice(&match kind {
            "zero" => [0, 0],
            "dc" => [123_456, -123_456],
            "mixed" => [123_456, noise[1]],
            "noise" => noise,
            "identical" => [varying, varying],
            "opposite" => [varying, -varying],
            "hard_left" => [varying, 0],
            "hard_right" => [0, varying],
            "extremes" => {
                if index % 2 == 0 {
                    [-8_388_608, 8_388_352]
                } else {
                    [8_388_352, -8_388_608]
                }
            }
            "correlated" => [varying, varying + (noise[1] >> 12)],
            _ => panic!("unknown benchmark input"),
        });
    }
}
