// ABOUTME: Checks the `exact=1` listener policy flag against the strict buffer and tag parsers.
// ABOUTME: Keeps every live route unchanged and rejects malformed flags like other bad queries.
use esp_shazam_core::stream_buffer::{BufferPreset, OutputFormat};
use esp_shazam_core::stream_http::{
    request_listener_policy, request_network_generation, request_sonos_intent, route_for, Format,
    ListenerPolicy, Route, Status,
};

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
