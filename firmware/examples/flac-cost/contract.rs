// ABOUTME: Checks frame-size and reusable-output contracts shared by the FLAC cost benchmark and host tests.
// ABOUTME: Keeps the benchmark's zero/DC assertions tied to frames produced by the current encoder.

use esp_shazam_core::flac::MAX_FRAME_BYTES;

pub fn assert_frame(kind: &str, frame: &Vec<u8>, initial_ptr: *const u8, initial_capacity: usize) {
    assert!(frame.len() <= MAX_FRAME_BYTES);
    if kind == "zero" || kind == "dc" {
        assert_eq!(frame.len(), 14, "constant block at frame number zero");
    }
    assert_eq!(frame.as_ptr(), initial_ptr, "output storage must be reused");
    assert_eq!(frame.capacity(), initial_capacity);
}
