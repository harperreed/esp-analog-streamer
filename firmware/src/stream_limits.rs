// ABOUTME: Sizes encoded history independently of socket write deadlines.
// ABOUTME: Recognition keeps its bounded profile while capture remains unchanged.

pub const fn flac_history_frames(recognition_enabled: bool) -> usize {
    esp_shazam_core::stream_buffer::OutputFormat::Flac.history_frames(recognition_enabled)
}

pub const fn flac_write_limit_ms(recognition_enabled: bool) -> u64 {
    esp_shazam_core::stream_buffer::OutputFormat::Flac.write_limit_ms(recognition_enabled)
}

/// Exactly 12 seconds of 24 ms MP3 frames, independent of recognition's FLAC profile.
pub const fn mp3_history_frames() -> usize {
    esp_shazam_core::stream_buffer::OutputFormat::Mp3.history_frames(false)
}

/// One whole High-preset interval; history does not extend a blocked socket's deadline.
pub const fn mp3_write_limit_ms() -> u64 {
    esp_shazam_core::stream_buffer::OutputFormat::Mp3.write_limit_ms(false)
}
