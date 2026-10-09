// ABOUTME: Sizes encoded history independently of socket write deadlines.
// ABOUTME: Recognition keeps its bounded profile while capture remains unchanged.

#[cfg(any(test, not(feature = "mp3-stream")))]
pub const fn flac_history_frames(recognition_enabled: bool) -> usize {
    if recognition_enabled {
        24
    } else {
        141
    }
}

#[cfg(any(test, not(feature = "mp3-stream")))]
pub const fn flac_write_limit_ms(recognition_enabled: bool) -> u64 {
    if recognition_enabled {
        2_048
    } else {
        8_021
    }
}

/// Exactly 12 seconds of 24 ms MP3 frames, independent of recognition's FLAC profile.
#[cfg(any(test, feature = "mp3-stream"))]
pub const fn mp3_history_frames() -> usize {
    500
}

/// One whole High-preset interval; history does not extend a blocked socket's deadline.
#[cfg(any(test, feature = "mp3-stream"))]
pub const fn mp3_write_limit_ms() -> u64 {
    8_016
}
