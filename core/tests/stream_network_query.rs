// ABOUTME: Checks network generation query validation and buffered stream routing.
// ABOUTME: Keeps direct diagnostic URLs valid while rejecting ambiguous generation tags.
use esp_shazam_core::stream_buffer::{BufferPreset, OutputFormat};
use esp_shazam_core::stream_http::{route_for, Route, Status, StreamRequest};
#[test]
fn generation_and_buffer_share_a_strict_query() {
    for query in [
        "buffer=med&network_generation=42",
        "network_generation=42&buffer=med",
    ] {
        let line = format!("GET /stream.mp3?{query} HTTP/1.1");
        assert_eq!(StreamRequest::parse(&line).unwrap().generation, Some(42));
        assert_eq!(
            route_for(&line, OutputFormat::Mp3),
            Route::Buffered(BufferPreset::Medium)
        );
    }
    assert_eq!(
        StreamRequest::parse("GET /stream.wav HTTP/1.1")
            .unwrap()
            .generation,
        None
    );
}
#[test]
fn malformed_generation_and_extra_buffer_fields_are_rejected() {
    for query in [
        "network_generation=",
        "network_generation=-1",
        "network_generation=+1",
        "network_generation=4294967296",
        "network_generation=1&network_generation=2",
    ] {
        let line = format!("GET /stream.mp3?{query} HTTP/1.1");
        assert_eq!(
            StreamRequest::parse(&line),
            Err(Status::BadRequest),
            "{query}"
        );
        assert_eq!(
            route_for(&line, OutputFormat::Mp3),
            Route::Refuse(Status::BadRequest)
        );
    }
    assert_eq!(
        route_for(
            "GET /stream.mp3?buffer=med&network_generation=2&other=1 HTTP/1.1",
            OutputFormat::Mp3
        ),
        Route::Refuse(Status::BadRequest)
    );
}
