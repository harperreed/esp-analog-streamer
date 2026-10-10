// ABOUTME: Bounded responses and reports for the board's one-item Sonos queue.
// ABOUTME: The sole media URL comes from the board address, never a request parameter.

use std::net::Ipv4Addr;

pub const QUEUE_PATH: &str = "/api/sonos/queue/v2.1";
pub const MAX_REPORT_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSnapshot {
    pub item_id: String,
    pub host: Ipv4Addr,
    pub buffer_preset: Option<crate::stream_buffer::BufferPreset>,
    pub format: crate::stream_buffer::OutputFormat,
}

#[derive(Debug, Clone, Copy)]
pub enum Endpoint {
    Context,
    Version,
    ItemWindow,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Invalid,
    StaleItem,
}

impl QueueSnapshot {
    pub fn new(
        item_id: String,
        host: Ipv4Addr,
        buffer_preset: Option<crate::stream_buffer::BufferPreset>,
        format: crate::stream_buffer::OutputFormat,
    ) -> Result<Self, Error> {
        crate::sonos::validate_id(&item_id).map_err(|_| Error::Invalid)?;
        Ok(Self {
            item_id,
            host,
            buffer_preset,
            format,
        })
    }
}

/// Empty itemId asks for the only item; a supplied ID must match exactly.
/// Window hints and old version tokens do not alter the one-item response.
pub fn response(
    snapshot: &QueueSnapshot,
    endpoint: Endpoint,
    query: &str,
) -> Result<String, Error> {
    use serde_json::json;
    crate::sonos::validate_id(&snapshot.item_id).map_err(|_| Error::Invalid)?;
    if query.len() > 1024 {
        return Err(Error::TooLarge);
    }
    let mut found_item = false;
    for (index, pair) in query.split('&').filter(|p| !p.is_empty()).enumerate() {
        if index >= 32 {
            return Err(Error::TooLarge);
        }
        let (key, value) = pair.split_once('=').ok_or(Error::Invalid)?;
        let key = decode_query(key)?;
        let value = decode_query(value)?;
        if key == "itemId" {
            if found_item {
                return Err(Error::Invalid);
            }
            found_item = true;
            if !value.is_empty() && value != snapshot.item_id {
                return Err(Error::StaleItem);
            }
        }
    }
    let item = &snapshot.item_id;
    let mut body = json!({"contextVersion":item, "queueVersion":item});
    match endpoint {
        Endpoint::Version => (),
        Endpoint::Context => {
            body["container"] =
                json!({"type":"playlist","name":"ESP Analog Streamer","id":{"objectId":item}});
            body["playbackPolicies"] = json!({"canRepeat":false,"canRepeatOne":false,"canShuffle":false,"canCrossfade":false});
            body["reports"] = json!({"sendUpdateAfterMillis":1000,"periodicIntervalMillis":10000});
        }
        Endpoint::ItemWindow => {
            body["includesBeginningOfQueue"] = json!(true);
            body["includesEndOfQueue"] = json!(true);
            body["items"] = json!([{"id":item,"track":{"type":"track","name":"ESP Analog Streamer","mediaUrl":media_url(snapshot),"contentType":snapshot.format.format().content_type()}}]);
        }
    }
    // All fields are bounded above; no input-supplied collections enter this body.
    serde_json::to_string(&body).map_err(|_| Error::Invalid)
}

fn media_url(snapshot: &QueueSnapshot) -> String {
    let url = format!(
        "http://{}:8000{}",
        snapshot.host,
        snapshot.format.format().path()
    );
    match snapshot.buffer_preset {
        Some(preset) => format!("{url}?buffer={}", preset.query_value()),
        None => url,
    }
}

fn decode_query(value: &str) -> Result<String, Error> {
    let mut input = value.bytes();
    let mut decoded = Vec::with_capacity(value.len());
    while let Some(byte) = input.next() {
        decoded.push(match byte {
            b'+' => b' ',
            b'%' => {
                let hi = (input.next().ok_or(Error::Invalid)? as char)
                    .to_digit(16)
                    .ok_or(Error::Invalid)?;
                let lo = (input.next().ok_or(Error::Invalid)? as char)
                    .to_digit(16)
                    .ok_or(Error::Invalid)?;
                (hi * 16 + lo) as u8
            }
            _ => byte,
        });
    }
    String::from_utf8(decoded).map_err(|_| Error::Invalid)
}

/// Acknowledge one current-item report. Reports cannot update the queue or URL.
pub fn validate_time_played(snapshot: &QueueSnapshot, body: &[u8]) -> Result<(), Error> {
    if body.len() > MAX_REPORT_BYTES {
        return Err(Error::TooLarge);
    }
    // Limit nesting before serde visits ignored metadata, as well as total bytes.
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for &byte in body {
        if escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        } else if !quoted {
            match byte {
                b'[' | b'{' => {
                    depth += 1;
                    if depth > 16 {
                        return Err(Error::TooLarge);
                    }
                }
                b']' | b'}' => {
                    depth = depth.checked_sub(1).ok_or(Error::Invalid)?;
                }
                _ => (),
            }
        }
    }
    #[derive(serde::Deserialize)]
    struct Report {
        items: [PlayedItem; 1],
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PlayedItem {
        id: String,
        context_version: String,
        queue_version: String,
        media_url: String,
    }
    let report: Report = serde_json::from_slice(body).map_err(|_| Error::Invalid)?;
    let item = &report.items[0];
    if [&item.id, &item.context_version, &item.queue_version]
        .into_iter()
        .any(|id| id != &snapshot.item_id)
        || item.media_url != media_url(snapshot)
    {
        return Err(Error::StaleItem);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn snapshot() -> QueueSnapshot {
        QueueSnapshot {
            item_id: "esp-test-1".into(),
            host: Ipv4Addr::new(192, 0, 2, 10),
            buffer_preset: None,
            format: crate::stream_buffer::OutputFormat::Flac,
        }
    }

    fn test_live_url() -> String {
        let path = "/stream.flac";
        format!("http://192.0.2.10:8000{path}")
    }

    fn report() -> Value {
        json!({"items":[{"contextVersion":"esp-test-1", "queueVersion":"esp-test-1", "id":"esp-test-1", "mediaUrl":test_live_url(), "type":"update", "positionMillis":2500, "positionMillisAtSegmentStart":0, "durationPlayedMillis":2500, "timeSincePlaybackMillis":2500}]})
    }

    #[test]
    fn queue_and_reports_use_the_selected_compressed_codec() {
        let (path, content_type, inactive) = ("/stream.flac", "audio/flac", "/stream.mp3");
        let snapshot = QueueSnapshot::new(
            "esp-test-1".into(),
            snapshot().host,
            Some(crate::stream_buffer::BufferPreset::Medium),
            crate::stream_buffer::OutputFormat::Flac,
        )
        .unwrap();
        let url = format!("http://192.0.2.10:8000{path}?buffer=med");
        let window: Value =
            serde_json::from_str(&response(&snapshot, Endpoint::ItemWindow, "").unwrap()).unwrap();
        assert_eq!(window["items"][0]["track"]["mediaUrl"], url);
        assert_eq!(window["items"][0]["track"]["contentType"], content_type);
        let mut played = report();
        played["items"][0]["mediaUrl"] = json!(url);
        assert_eq!(
            validate_time_played(&snapshot, played.to_string().as_bytes()),
            Ok(())
        );
        played["items"][0]["mediaUrl"] =
            json!(format!("http://192.0.2.10:8000{inactive}?buffer=med"));
        assert_eq!(
            validate_time_played(&snapshot, played.to_string().as_bytes()),
            Err(Error::StaleItem)
        );
    }

    #[test]
    fn buffered_snapshot_url_is_fixed_and_reports_require_that_exact_url() {
        for (preset, value) in [
            (crate::stream_buffer::BufferPreset::Low, "low"),
            (crate::stream_buffer::BufferPreset::Medium, "med"),
            (crate::stream_buffer::BufferPreset::High, "high"),
        ] {
            let s = QueueSnapshot::new(
                "esp-test-1".into(),
                snapshot().host,
                Some(preset),
                crate::stream_buffer::OutputFormat::Flac,
            )
            .unwrap();
            let expected = format!("{}?buffer={value}", test_live_url());
            let body: Value = serde_json::from_str(
                &response(
                    &s,
                    Endpoint::ItemWindow,
                    "buffer=high&mediaUrl=http%3A%2F%2Fevil",
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(body["items"][0]["track"]["mediaUrl"], expected);
            assert_eq!(
                validate_time_played(&s, report().to_string().as_bytes()),
                Err(Error::StaleItem)
            );
            let mut played = report();
            played["items"][0]["mediaUrl"] = json!(expected);
            assert_eq!(
                validate_time_played(&s, played.to_string().as_bytes()),
                Ok(())
            );
            let clone = s.clone();
            assert_eq!(
                response(&clone, Endpoint::ItemWindow, "").unwrap(),
                response(&s, Endpoint::ItemWindow, "").unwrap()
            );
        }
    }

    #[test]
    fn emits_proven_context_and_version_contract() {
        let context: Value = serde_json::from_str(
            &response(
                &snapshot(),
                Endpoint::Context,
                "queueVersion=&contextVersion=",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            context,
            json!({"contextVersion":"esp-test-1", "queueVersion":"esp-test-1", "container":{"type":"playlist","name":"ESP Analog Streamer","id":{"objectId":"esp-test-1"}}, "playbackPolicies":{"canRepeat":false,"canRepeatOne":false,"canShuffle":false,"canCrossfade":false}, "reports":{"sendUpdateAfterMillis":1000,"periodicIntervalMillis":10000}})
        );
        let version: Value = serde_json::from_str(
            &response(
                &snapshot(),
                Endpoint::Version,
                "updateToken=true&queueVersion=&contextVersion=previous",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            version,
            json!({"contextVersion":"esp-test-1", "queueVersion":"esp-test-1"})
        );
    }

    #[test]
    fn item_window_has_one_live_track_without_duration_or_request_supplied_url() {
        let query = "isExplicit=false&reason=load&itemId=esp-test-1&positionMillis=0&queueVersion=esp-test-1&contextVersion=esp-test-1&previousWindowSize=9&upcomingWindowSize=10&heardItemId=&mediaUrl=http%3A%2F%2Fevil.invalid";
        let window: Value =
            serde_json::from_str(&response(&snapshot(), Endpoint::ItemWindow, query).unwrap())
                .unwrap();
        assert_eq!(
            window,
            json!({"contextVersion":"esp-test-1","queueVersion":"esp-test-1","includesBeginningOfQueue":true,"includesEndOfQueue":true,"items":[{"id":"esp-test-1","track":{"type":"track","name":"ESP Analog Streamer","mediaUrl":test_live_url(),"contentType":crate::stream_http::Format::Flac.content_type()}}]})
        );
        assert!(response(&snapshot(), Endpoint::ItemWindow, "").is_ok());
        assert!(response(&snapshot(), Endpoint::ItemWindow, "itemId=").is_ok());
    }

    #[test]
    fn rejects_stale_duplicate_malformed_and_oversized_query_ids() {
        for query in [
            "itemId=old",
            "itemId=esp-test-1&itemId=old",
            "itemId=%",
            "itemId=%gg",
            "itemId=%ff",
            "itemId=esp-test-1%00",
        ] {
            assert!(response(&snapshot(), Endpoint::ItemWindow, query).is_err());
        }
        assert!(response(&snapshot(), Endpoint::Context, &"x".repeat(1025)).is_err());
        assert!(response(&snapshot(), Endpoint::Version, &"x=y&".repeat(33)).is_err());
        assert!(response(&snapshot(), Endpoint::ItemWindow, "item%49d=esp-test-%31").is_ok());
        assert!(matches!(
            response(&snapshot(), Endpoint::ItemWindow, "item%49d=stale"),
            Err(Error::StaleItem)
        ));
    }

    #[test]
    fn snapshot_validation_bounds_output_and_escapes_ids() {
        assert!(QueueSnapshot::new(
            String::new(),
            snapshot().host,
            None,
            crate::stream_buffer::OutputFormat::Flac
        )
        .is_err());
        assert!(QueueSnapshot::new(
            "x".repeat(257),
            snapshot().host,
            None,
            crate::stream_buffer::OutputFormat::Flac
        )
        .is_err());
        let s = QueueSnapshot::new(
            "\"".repeat(256),
            snapshot().host,
            None,
            crate::stream_buffer::OutputFormat::Flac,
        )
        .unwrap();
        let encoded = response(&s, Endpoint::ItemWindow, "").unwrap();
        assert!(encoded.len() < 4096);
        let v: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(v["items"][0]["id"], s.item_id);
        assert!(response(
            &QueueSnapshot {
                item_id: "x".repeat(257),
                ..snapshot()
            },
            Endpoint::Context,
            ""
        )
        .is_err());
    }

    #[test]
    fn acknowledges_only_reports_for_current_item_versions_and_media() {
        assert_eq!(
            validate_time_played(&snapshot(), report().to_string().as_bytes()),
            Ok(())
        );
        let mut final_report = report();
        final_report["items"][0]["type"] = json!("final");
        assert_eq!(
            validate_time_played(&snapshot(), final_report.to_string().as_bytes()),
            Ok(())
        );
        for field in ["id", "contextVersion", "queueVersion", "mediaUrl"] {
            let mut body = report();
            body["items"][0][field] = json!("stale");
            assert_eq!(
                validate_time_played(&snapshot(), body.to_string().as_bytes()),
                Err(Error::StaleItem)
            );
        }
    }

    #[test]
    fn reports_are_bounded_before_parse_and_reject_bad_shapes() {
        for body in [
            "{}",
            "{",
            r#"{"items":[]}"#,
            r#"{"items":[{},{}]}"#,
            r#"{"items":[{"id":"esp-test-1"}]}"#,
        ] {
            assert!(validate_time_played(&snapshot(), body.as_bytes()).is_err());
        }
        assert_eq!(
            validate_time_played(&snapshot(), &vec![b' '; 1025]),
            Err(Error::TooLarge)
        );
        let mut bytes = report().to_string().into_bytes();
        bytes.resize(1024, b' ');
        assert_eq!(validate_time_played(&snapshot(), &bytes), Ok(()));
        let nested = format!("{{\"x\":{}0{}}}", "[".repeat(40), "]".repeat(40));
        assert!(validate_time_played(&snapshot(), nested.as_bytes()).is_err());
    }
}
