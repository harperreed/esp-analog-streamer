// ABOUTME: Sizes encoded FLAC history independently of socket write deadlines.
// ABOUTME: Recognition keeps its bounded profile while capture remains unchanged.

pub const fn flac_history_frames(recognition_enabled: bool) -> usize {
    if recognition_enabled {
        24
    } else {
        141
    }
}

pub const fn flac_write_limit_ms(recognition_enabled: bool) -> u64 {
    if recognition_enabled {
        2_048
    } else {
        8_021
    }
}
