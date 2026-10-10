// ABOUTME: Checks codec-exclusive HTTP contracts and whole-frame startup targets.
// ABOUTME: Runs against both firmware codec profiles without a device.
use esp_shazam_core::{
    stream_buffer::{BufferPreset, OutputFormat},
    stream_http::{route_for, Format, Route, Status},
};

#[test]
fn presets_round_up_for_each_encoder_block_size() {
    for (preset, flac, mp3) in [
        (BufferPreset::Low, 12, 42),
        (BufferPreset::Medium, 47, 167),
        (BufferPreset::High, 94, 334),
    ] {
        assert_eq!(preset.frames(4096), flac);
        assert_eq!(preset.frames(1152), mp3);
    }
}

#[test]
fn only_the_selected_compressed_codec_is_routed() {
    for selected in [OutputFormat::Flac, OutputFormat::Mp3] {
        let route = |line: &str| route_for(line, selected);
        let active = selected.format();
        let inactive = if active == Format::Flac {
            Format::Mp3
        } else {
            Format::Flac
        };
        assert_eq!(
            route(&format!("GET {} HTTP/1.1", active.path())),
            Route::Stream(active)
        );
        assert_eq!(
            route(&format!("GET {} HTTP/1.1", inactive.path())),
            Route::Refuse(Status::NotFound)
        );
        assert_eq!(
            route(&format!("GET {}?buffer=med HTTP/1.1", active.path())),
            Route::Buffered(BufferPreset::Medium)
        );
        assert_eq!(
            route(&format!("GET {}?buffer=med HTTP/1.1", inactive.path())),
            Route::Refuse(Status::NotFound)
        );
        assert!(active.response_head().contains(active.content_type()));
        assert_eq!(Format::Mp3.content_type(), "audio/mpeg");
        assert_eq!(Format::Mp3.path(), "/stream.mp3");
    }
}
