// ABOUTME: Checks the `exact=1` listener policy flag against the strict buffer and tag parsers.
// ABOUTME: Keeps every live route unchanged and rejects malformed flags like other bad queries.
use esp_shazam_core::stream_buffer::{BufferPreset, OutputFormat};
use esp_shazam_core::stream_http::{
    request_listener_policy, request_network_generation, request_sonos_intent, route_for, Format,
    ListenerPolicy, Method, Route, Status, StreamRequest,
};

#[test]
fn one_parse_holds_every_recognised_key_and_tolerates_each_beside_buffer() {
    // The keys the parser knows are the keys that may ride beside `buffer=`: a well-formed value
    // keeps the buffered route, a malformed value of the same key is refused, and a key the
    // parser does not know beside `buffer=` is refused too. One list serves both rules.
    for (key, value) in [
        ("network_generation", "7"),
        ("sonos_intent", "41"),
        ("exact", "1"),
    ] {
        let line = format!("GET /stream.flac?buffer=med&{key}={value} HTTP/1.1");
        let request = StreamRequest::parse(&line).unwrap();
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.path, "/stream.flac");
        assert_eq!(request.buffer, Some(BufferPreset::Medium));
        assert_eq!(
            request.route(OutputFormat::Flac),
            Route::Buffered(BufferPreset::Medium),
            "{line}"
        );
        let line = format!("GET /stream.flac?buffer=med&{key}=x HTTP/1.1");
        assert_eq!(
            StreamRequest::parse(&line),
            Err(Status::BadRequest),
            "{line}"
        );
    }
    assert_eq!(
        StreamRequest::parse("GET /stream.flac?buffer=med&cache=1 HTTP/1.1"),
        Err(Status::BadRequest)
    );
    let request = StreamRequest::parse("GET /stream.flac?cache=1&exact=1 HTTP/1.1").unwrap();
    assert_eq!(request.policy, ListenerPolicy::Exact);
    assert_eq!(
        request.route(OutputFormat::Flac),
        Route::Stream(Format::Flac)
    );
    // A malformed query is refused whatever codec is active; the line and method come first.
    assert_eq!(
        StreamRequest::parse("GET /stream.mp3?exact=2 HTTP/1.1"),
        Err(Status::BadRequest)
    );
    assert_eq!(
        StreamRequest::parse("POST /stream.flac?exact=2 HTTP/1.1"),
        Err(Status::MethodNotAllowed)
    );
    assert_eq!(
        StreamRequest::parse("GET /stream.flac?exact=2 HTTP/2"),
        Err(Status::BadRequest)
    );
}

#[test]
fn exact_flag_combines_with_buffer_and_tags_without_changing_the_route() {
    let tags = [
        "",
        "network_generation=7",
        "sonos_intent=41&network_generation=7",
    ];
    for format in [OutputFormat::Flac, OutputFormat::Mp3] {
        let path = format.format().path();
        for tag in tags {
            for (buffer, expected) in [
                ("", Route::Stream(format.format())),
                ("buffer=med", Route::Buffered(BufferPreset::Medium)),
            ] {
                for exact in [false, true] {
                    let fields: Vec<&str> = [if exact { "exact=1" } else { "" }, buffer, tag]
                        .into_iter()
                        .filter(|field| !field.is_empty())
                        .collect();
                    let query = if fields.is_empty() {
                        String::new()
                    } else {
                        format!("?{}", fields.join("&"))
                    };
                    let line = format!("GET {path}{query} HTTP/1.1");
                    assert_eq!(route_for(&line, format), expected, "{line}");
                    assert_eq!(
                        request_listener_policy(&line).unwrap(),
                        if exact {
                            ListenerPolicy::Exact
                        } else {
                            ListenerPolicy::Live
                        },
                        "{line}"
                    );
                    // The flag must not disturb the tag parsers the Sonos route check relies on.
                    assert_eq!(
                        request_network_generation(&line).unwrap(),
                        (!tag.is_empty()).then_some(7)
                    );
                    assert_eq!(
                        request_sonos_intent(&line).unwrap(),
                        tag.starts_with("sonos_intent").then_some(41)
                    );
                    assert_eq!(
                        route_for(&line, format).with_buffer_availability(false),
                        if buffer.is_empty() {
                            expected
                        } else {
                            Route::Refuse(Status::ServiceUnavailable)
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn malformed_exact_flags_are_refused_like_other_bad_queries() {
    for query in [
        "exact=0",
        "exact=2",
        "exact",
        "exact=",
        "exact=1&exact=1",
        "exact=01",
    ] {
        for buffer in ["", "&buffer=med"] {
            let line = format!("GET /stream.mp3?{query}{buffer} HTTP/1.1");
            assert!(request_listener_policy(&line).is_err(), "{line}");
            assert_eq!(
                route_for(&line, OutputFormat::Mp3),
                Route::Refuse(Status::BadRequest),
                "{line}"
            );
        }
    }
}

#[test]
fn wav_and_untagged_requests_default_to_live_and_cache_busters_stay_live() {
    for line in [
        "GET /stream.wav HTTP/1.1",
        "GET /stream.wav?nocache=1 HTTP/1.1",
        "GET /stream.mp3?nocache=1 HTTP/1.1",
        "GET /stream.mp3 HTTP/1.1",
    ] {
        assert_eq!(request_listener_policy(line).unwrap(), ListenerPolicy::Live);
    }
    assert_eq!(
        route_for("GET /stream.wav?nocache=1 HTTP/1.1", OutputFormat::Mp3),
        Route::Stream(Format::Wav)
    );
    // WAV is exact by construction; a well-formed flag on it is accepted and ignored.
    assert_eq!(
        route_for("GET /stream.wav?exact=1 HTTP/1.1", OutputFormat::Mp3),
        Route::Stream(Format::Wav)
    );
    assert_eq!(
        route_for(
            "GET /stream.mp3?exact=1&other=1 HTTP/1.1",
            OutputFormat::Mp3
        ),
        Route::Stream(Format::Mp3)
    );
    assert_eq!(
        route_for(
            "GET /stream.mp3?buffer=med&exact=1&other=1 HTTP/1.1",
            OutputFormat::Mp3
        ),
        Route::Refuse(Status::BadRequest)
    );
}
