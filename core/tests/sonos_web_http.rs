// ABOUTME: Host tests for the firmware's bounded Sonos HTTP body reader.
// ABOUTME: Real loopback sockets exercise fragmented bodies and absolute read deadlines.

#[path = "../../firmware/src/web.rs"]
mod web;

use esp_shazam_core::sonos_web::Action;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};
use web::{read_action, RequestError};

#[test]
fn rejects_ambiguous_missing_and_oversized_lengths_before_reading() {
    for (length, transfer, expected) in [
        (None, None, RequestError::LengthRequired),
        (Some("513"), None, RequestError::TooLarge),
        (Some("0"), None, RequestError::BadRequest),
        (Some("+2"), None, RequestError::BadRequest),
        (Some("2,2"), None, RequestError::BadRequest),
        (Some(" 2"), None, RequestError::BadRequest),
        (Some("2"), Some("chunked"), RequestError::BadRequest),
        (Some("2"), Some(""), RequestError::BadRequest),
    ] {
        let result = read_action(
            Some("application/json"),
            length,
            transfer,
            Duration::from_secs(1),
            |_, _| panic!("invalid framing read a body"),
        );
        assert_eq!(result, Err(expected));
    }
}

#[test]
fn parses_fragmented_action_and_rejects_incomplete_or_extra_fields() {
    let body = br#"{"action":"select","player_id":"RINCON_TEST"}"#;
    let mut bytes = body.as_slice();
    assert_eq!(
        read_action(
            Some("application/json"),
            Some(&body.len().to_string()),
            None,
            Duration::from_secs(1),
            |out, _| bytes.read(&mut out[..1])
        ),
        Ok(Action::Select {
            player_id: "RINCON_TEST".into()
        })
    );
    let mut bytes = b"{".as_slice();
    assert_eq!(
        read_action(
            Some("application/json"),
            Some("2"),
            None,
            Duration::from_secs(1),
            |out, _| bytes.read(out)
        ),
        Err(RequestError::BadRequest)
    );
    let mut bytes = br#"{"action":"play","url":"http://example.com"}"#.as_slice();
    assert_eq!(
        read_action(
            Some("application/json"),
            Some(&bytes.len().to_string()),
            None,
            Duration::from_secs(1),
            |out, _| bytes.read(out)
        ),
        Err(RequestError::BadRequest)
    );
}

#[test]
fn exact_512_byte_body_is_accepted_and_zero_budget_reads_nothing() {
    let mut body = br#"{"action":"play"}"#.to_vec();
    body.resize(512, b' ');
    let mut bytes = body.as_slice();
    assert_eq!(
        read_action(
            Some("application/json"),
            Some("512"),
            None,
            Duration::from_secs(1),
            |out, _| bytes.read(out)
        ),
        Ok(Action::Play {})
    );
    assert_eq!(
        read_action(
            Some("application/json"),
            Some("2"),
            None,
            Duration::ZERO,
            |_, _| panic!("expired reader called")
        ),
        Err(RequestError::Timeout)
    );
}

#[test]
fn real_slow_drip_cannot_extend_absolute_body_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    let producer = std::thread::spawn(move || {
        let mut client = client;
        for _ in 0..20 {
            if client.write_all(b" ").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let start = Instant::now();
    let result = read_action(
        Some("application/json"),
        Some("512"),
        None,
        Duration::from_millis(100),
        |out, remaining| {
            server.set_read_timeout(Some(remaining))?;
            server.read(out)
        },
    );
    assert_eq!(result, Err(RequestError::Timeout));
    assert!(
        start.elapsed() < Duration::from_millis(350),
        "drip reset the body deadline"
    );
    drop(server);
    producer.join().unwrap();
}

#[test]
fn only_json_content_type_can_reach_body_reader() {
    for content_type in [
        None,
        Some("text/plain"),
        Some("application/x-www-form-urlencoded"),
        Some("multipart/form-data; boundary=x"),
        Some("application/jsonp"),
        Some("application/json; charset=latin1"),
    ] {
        assert_eq!(
            read_action(
                content_type,
                Some("2"),
                None,
                Duration::from_secs(1),
                |_, _| panic!("unsupported content type read body")
            ),
            Err(RequestError::UnsupportedMediaType)
        );
    }
    for content_type in [
        "application/json",
        "application/json; charset=utf-8",
        "Application/JSON; Charset=\"UTF-8\"",
    ] {
        let mut bytes = br#"{"action":"stop"}"#.as_slice();
        assert_eq!(
            read_action(
                Some(content_type),
                Some(&bytes.len().to_string()),
                None,
                Duration::from_secs(1),
                |out, _| bytes.read(out)
            ),
            Ok(Action::Stop {})
        );
    }
}
