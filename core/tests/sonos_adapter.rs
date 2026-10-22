// ABOUTME: Runs the firmware Sonos adapter's host-safe boundary tests in canonical checks.
// ABOUTME: Exercises the HTTP transport-state mapping and the owned-URI check.

#[path = "../../firmware/src/sonos.rs"]
pub mod sonos;

#[test]
fn stopped_transport_ends_stream_intent() {
    assert_eq!(
        sonos::http_transport_state(esp_shazam_core::sonos_upnp::TransportState::Stopped),
        sonos::State::Stopped
    );
    assert_eq!(
        sonos::http_transport_state(esp_shazam_core::sonos_upnp::TransportState::Paused),
        sonos::State::Stopped
    );
}

#[test]
fn foreign_uri_is_ineligible_for_checked_stop() {
    let ownership = sonos::HttpOwnership::new("http://board/stream.mp3?intent=1".into());
    assert!(ownership.matches("http://board/stream.mp3?intent=1"));
    assert!(!ownership.matches("http://board/stream.mp3?intent=2"));
}
