#![no_main]
// ABOUTME: Drives the real bounded HTTP reader and both active-codec routers.
// ABOUTME: Preserves permissive framing while checking consumption and repeatability.
use esp_shazam_core::{
    stream_buffer::OutputFormat,
    stream_http::{self, Route, Status},
};
use esp_shazam_fuzz::{assert_head_contract, assert_route_contract};
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;
fuzz_target!(|input: &[u8]| {
    if input.len() < 2 {
        return;
    }
    let cap = u64::from(u16::from_le_bytes([input[0], input[1]])) % 4097;
    let data = &input[2..input.len().min(4098)];
    let mut cursor = Cursor::new(data);
    let result = stream_http::read_request_line(&mut cursor, cap);
    assert!(cursor.position() <= cap && cursor.position() <= data.len() as u64);
    let mut repeat = Cursor::new(data);
    let again = stream_http::read_request_line(&mut repeat, cap);
    assert_eq!(cursor.position(), repeat.position());
    assert_eq!(
        result.as_ref().map_err(|e| e.kind()),
        again.as_ref().map_err(|e| e.kind())
    );
    assert_head_contract(data, cap, cursor.position(), &result);
    let direct = std::str::from_utf8(data).ok();
    for line in result.as_deref().ok().into_iter().chain(direct) {
        if result.as_deref().ok() == Some(line) {
            assert!(data[..cursor.position() as usize]
                .windows(line.len())
                .any(|w| w == line.as_bytes()));
        }
        for active in [OutputFormat::Flac, OutputFormat::Mp3] {
            let route = stream_http::route_for(line, active);
            assert_eq!(route, stream_http::route_for(line, active));
            assert_route_contract(line, active, route);
            assert_eq!(route.with_buffer_availability(true), route);
            if matches!(route, Route::Buffered(_)) {
                assert_eq!(
                    route.with_buffer_availability(false),
                    Route::Refuse(Status::ServiceUnavailable)
                );
            }
            if let Route::Refuse(status) = route {
                assert!(status.response().starts_with("HTTP/1.1 "));
            }
        }
    }
});
