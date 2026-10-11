// ABOUTME: Runtime compressed format settings and frozen queue contracts.
// ABOUTME: Exercises public serde validation before device-specific ownership tests.
use esp_shazam_core::{sonos::SonosConfig, sonos_web};

#[test]
fn default_format_is_flac_and_settings_preserve_buffer() {
    let config: SonosConfig =
        serde_json::from_str(r#"{"player_id":null,"autoplay":false,"buffer_preset":"high"}"#)
            .unwrap();
    let value = serde_json::to_value(config).unwrap();
    assert_eq!(value["output_format"], "flac");
    for format in ["flac", "mp3"] {
        let config: SonosConfig = serde_json::from_value(serde_json::json!({"player_id":null,"autoplay":false,"buffer_preset":"high","output_format":format})).unwrap();
        let value = serde_json::to_value(config).unwrap();
        assert_eq!(value["output_format"], format);
        assert_eq!(value["buffer_preset"], "high");
    }
}
#[test]
fn format_action_accepts_only_compressed_formats() {
    for format in ["flac", "mp3"] {
        let body = format!(r#"{{"action":"format","format":"{format}"}}"#);
        assert!(sonos_web::parse(body.as_bytes()).is_ok());
    }
    for body in [
        r#"{"action":"format","format":"wav"}"#,
        r#"{"action":"format","format":"mp3","url":"evil"}"#,
    ] {
        assert!(sonos_web::parse(body.as_bytes()).is_err());
    }
    assert!(serde_json::from_str::<SonosConfig>(
        r#"{"player_id":null,"autoplay":false,"output_format":"wav"}"#
    )
    .is_err());
}

#[test]
fn queue_freezes_format_with_buffer() {
    use esp_shazam_core::{
        sonos_queue::{response, Endpoint, QueueSnapshot},
        stream_buffer::{BufferPreset, OutputFormat},
    };
    for (format, path, mime) in [
        (OutputFormat::Flac, "flac", "audio/flac"),
        (OutputFormat::Mp3, "mp3", "audio/mpeg"),
    ] {
        let snapshot = QueueSnapshot::new(
            "item".into(),
            "192.0.2.1".parse().unwrap(),
            Some(BufferPreset::High),
            format,
            0,
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_str(
            &response(&snapshot, Endpoint::ItemWindow, "format=flac&buffer=low").unwrap(),
        )
        .unwrap();
        assert_eq!(
            body["items"][0]["track"]["mediaUrl"],
            format!("http://192.0.2.1:8000/stream.{path}?buffer=high&network_generation=0")
        );
        assert_eq!(body["items"][0]["track"]["contentType"], mime);
    }
}
