// ABOUTME: Independent bounded checks shared by the real host fuzz targets.
// ABOUTME: Checks framing without duplicating production parsing or transitions.
#[cfg(test)]
mod tests {
    #[test]
    fn crc_rejects_corruption() {
        assert_eq!(super::crc(&[0xff, 0xf8, 0xca, 0x18, 0, 0x7f], 8, 0x07), 0);
        assert_ne!(super::crc(&[0xff, 0xf8, 0xca, 0x18, 1, 0x7f], 8, 0x07), 0);
    }
}
pub fn crc(bytes: &[u8], width: u32, polynomial: u16) -> u16 {
    let mut value = 0u16;
    for &byte in bytes {
        value ^= u16::from(byte) << (width - 8);
        for _ in 0..8 {
            let high = value & (1 << (width - 1)) != 0;
            value = value.wrapping_shl(1);
            if high {
                value ^= polynomial;
            }
        }
        if width == 8 {
            value &= 255;
        }
    }
    value
}
pub fn number(bytes: &[u8]) -> u32 {
    let count = match bytes[0].leading_ones() {
        0 => 1,
        n @ 2..=6 => n as usize,
        _ => panic!("invalid number"),
    };
    let mut value = if count == 1 {
        u32::from(bytes[0])
    } else {
        u32::from(bytes[0] & (0x7f >> count))
    };
    for &byte in &bytes[1..count] {
        assert_eq!(byte & 0xc0, 0x80);
        value = (value << 6) | u32::from(byte & 0x3f);
    }
    let minimum = [0, 0, 128, 2048, 65536, 2097152, 67108864][count];
    assert!(value >= minimum && value <= 0x7fffffff);
    value
}
/// The channel assignment the encoder chose for one block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assignment {
    /// Two 16-bit subframes: left, then right.
    Independent,
    /// A 16-bit left subframe, then a 17-bit side (left minus right) subframe.
    LeftSide,
}
/// Checks the fixed header fields, number, CRC-8 and CRC-16 of one encoder frame.
/// Returns where the subframes start and which assignment the header declares.
pub fn header(frame: &[u8]) -> (usize, Assignment) {
    assert_eq!(&frame[..3], &[0xff, 0xf8, 0xca]);
    let assignment = match frame[3] {
        0x18 => Assignment::Independent,
        0x88 => Assignment::LeftSide,
        byte => panic!("unexpected channel assignment byte {byte:#04x}"),
    };
    let count = match frame[4].leading_ones() {
        0 => 1,
        n @ 2..=6 => n as usize,
        _ => panic!("invalid number"),
    };
    number(&frame[4..4 + count]);
    let end = 5 + count;
    assert_eq!(crc(&frame[..end], 8, 7), 0);
    assert_eq!(crc(frame, 16, 0x8005), 0);
    // Two constant subframes: 24 bits each, 25 for the 17-bit side, rounded up to bytes.
    let minimum_subframes = match assignment {
        Assignment::Independent => 6,
        Assignment::LeftSide => 7,
    };
    assert!(frame.len() >= end + minimum_subframes + 2);
    (end, assignment)
}
#[cfg(test)]
mod encoded_tests {
    use super::*;
    use esp_shazam_core::flac::{self, Encoder};
    fn encoded_from(samples: &[i32]) -> Vec<u8> {
        let mut encoder = Encoder::try_new().unwrap();
        encoder.push(samples);
        let mut frame = Vec::with_capacity(flac::MAX_FRAME_BYTES);
        encoder.encode_block(0, &mut frame);
        frame
    }
    fn encoded() -> Vec<u8> {
        encoded_from(&vec![0; 8192])
    }
    fn decoded(frame: Vec<u8>) -> Vec<i32> {
        let stream: Vec<u8> = flac::stream_header().into_iter().chain(frame).collect();
        let mut reader = claxon::FlacReader::new(&stream[..]).unwrap();
        reader.samples().collect::<Result<_, _>>().unwrap()
    }
    #[test]
    fn independent_decoder_reads_complete_silent_block() {
        let frame = encoded();
        assert_eq!(header(&frame).1, Assignment::Independent);
        assert_eq!(decoded(frame), vec![0; 8192]);
    }
    #[test]
    fn independent_decoder_reads_left_side_block() {
        // Full-scale noise on both channels: the side channel is constant, so the encoder
        // picks left-side and the decoder must undo it to the same quantized samples.
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let samples: Vec<i32> = (0..4096)
            .flat_map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let sample = (state >> 40) as i32 - (1 << 23);
                [sample, sample]
            })
            .collect();
        let frame = encoded_from(&samples);
        assert_eq!(header(&frame).1, Assignment::LeftSide);
        let expected: Vec<i32> = samples
            .iter()
            .map(|&s| (s >> 8).clamp(-32768, 32767))
            .collect();
        assert_eq!(decoded(frame), expected);
    }
    #[test]
    fn oracle_rejects_corrupted_header_and_footer() {
        let frame = encoded();
        for position in [2, frame.len() - 1] {
            let mut broken = frame.clone();
            broken[position] ^= 1;
            assert!(std::panic::catch_unwind(|| header(&broken)).is_err());
        }
    }
    #[test]
    fn oracle_rejects_mid_side_assignment_with_valid_crcs() {
        let mut broken = encoded();
        let (end, _) = header(&broken);
        broken[3] = 0x28;
        broken[end - 1] = crc(&broken[..end - 1], 8, 7) as u8;
        let len = broken.len();
        let footer = crc(&broken[..len - 2], 16, 0x8005).to_be_bytes();
        broken[len - 2..].copy_from_slice(&footer);
        assert_eq!(crc(&broken[..end], 8, 7), 0);
        assert_eq!(crc(&broken, 16, 0x8005), 0);
        assert!(std::panic::catch_unwind(|| header(&broken)).is_err());
    }
}

#[cfg(test)]
mod number_tests {
    #[test]
    fn canonical_number_boundaries() {
        for (bytes, number) in [
            (&[127][..], 127),
            (&[0xc2, 0x80][..], 128),
            (&[0xdf, 0xbf][..], 2047),
            (&[0xe0, 0xa0, 0x80][..], 2048),
            (&[0xfd, 0xbf, 0xbf, 0xbf, 0xbf, 0xbf][..], 0x7fffffff),
        ] {
            assert_eq!(super::number(bytes), number);
        }
    }
    #[test]
    #[should_panic]
    fn rejects_noncanonical_number() {
        super::number(&[0xc0, 0x80]);
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use esp_shazam_core::{
        stream_buffer::OutputFormat,
        stream_http::{Route, Status},
    };
    #[test]
    fn route_oracle_checks_named_branches_and_rejects_wrong_status() {
        for active in [OutputFormat::Flac, OutputFormat::Mp3] {
            for line in [
                "GET /stream.flac HTTP/1.1",
                "GET /stream.mp3?buffer=bad HTTP/1.xyz",
                "GET /stream.wav?buffer=low HTTP/1.1",
                "GET /stream.flac?buffer=low&buffer=high HTTP/1.1",
                "GET /missing HTTP/1.1",
                "GET /stream.flac?cache=1 HTTP/1.1",
                "GET /stream.flac?buffer=high HTTP/1.1",
            ] {
                assert_route_contract(
                    line,
                    active,
                    esp_shazam_core::stream_http::route_for(line, active),
                );
                assert!(std::panic::catch_unwind(|| assert_route_contract(
                    line,
                    active,
                    Route::Refuse(Status::ServiceUnavailable)
                ))
                .is_err());
            }
        }
    }
    #[test]
    fn committed_http_seeds_match_independent_contracts() {
        for seed in [
            &include_bytes!("../corpus/http_head/live")[..],
            &include_bytes!("../corpus/http_head/buffered")[..],
            &include_bytes!("../corpus/http_head/duplicate-buffer")[..],
            &include_bytes!("../corpus/http_head/inactive")[..],
            &include_bytes!("../corpus/http_head/wav-buffer")[..],
            &include_bytes!("../corpus/http_head/unknown")[..],
            &include_bytes!("../corpus/http_head/permissive")[..],
            &include_bytes!("../corpus/http_head/truncated")[..],
            &include_bytes!("../corpus/http_head/invalid-utf8")[..],
            &include_bytes!("../corpus/http_head/cap")[..],
        ] {
            let cap = u64::from(u16::from_le_bytes([seed[0], seed[1]])) % 4097;
            let data = &seed[2..];
            let mut cursor = std::io::Cursor::new(data);
            let result = esp_shazam_core::stream_http::read_request_line(&mut cursor, cap);
            assert_head_contract(data, cap, cursor.position(), &result);
            if let Ok(line) = result {
                for active in [OutputFormat::Flac, OutputFormat::Mp3] {
                    assert_route_contract(
                        &line,
                        active,
                        esp_shazam_core::stream_http::route_for(&line, active),
                    );
                }
            }
        }
    }
    #[test]
    fn head_oracle_checks_truncation_and_consumed_utf8() {
        for data in [&b"GET /"[..], &b"GET /\xff HTTP/1.1\n\n"[..]] {
            let mut cursor = std::io::Cursor::new(data);
            let result = esp_shazam_core::stream_http::read_request_line(&mut cursor, 4096);
            assert_head_contract(data, 4096, cursor.position(), &result);
            assert!(std::panic::catch_unwind(|| assert_head_contract(
                data,
                4096,
                cursor.position(),
                &Ok("wrong".into())
            ))
            .is_err());
        }
    }
}

pub fn assert_head_contract(
    data: &[u8],
    cap: u64,
    consumed: u64,
    result: &std::io::Result<String>,
) {
    use std::io::ErrorKind;
    let consumed = &data[..consumed as usize];
    if std::str::from_utf8(consumed).is_err() {
        assert_eq!(result.as_ref().unwrap_err().kind(), ErrorKind::InvalidData);
    } else if let Err(error) = result {
        assert_eq!(
            error.kind(),
            if consumed.len() as u64 == cap {
                ErrorKind::InvalidData
            } else {
                ErrorKind::UnexpectedEof
            }
        );
    } else {
        assert!(consumed.ends_with(b"\n"));
        let last_line = consumed[..consumed.len() - 1]
            .rsplit(|&b| b == b'\n')
            .next()
            .unwrap();
        assert!(last_line.iter().all(|&b| b == b'\r'));
        assert!(!result.as_ref().unwrap().is_empty());
    }
}

pub fn assert_route_contract(
    line: &str,
    active: esp_shazam_core::stream_buffer::OutputFormat,
    route: esp_shazam_core::stream_http::Route,
) {
    use esp_shazam_core::{
        stream_buffer::BufferPreset,
        stream_http::{Format, Route, Status},
    };
    let tokens: Vec<_> = line.split_whitespace().take(4).collect();
    if tokens.len() != 3 || !tokens[1].starts_with('/') || !tokens[2].starts_with("HTTP/1.") {
        assert_eq!(route, Route::Refuse(Status::BadRequest));
        return;
    }
    if tokens[0] != "GET" {
        assert_eq!(route, Route::Refuse(Status::MethodNotAllowed));
        return;
    }
    // Fixed examples are independent expectations, not a second query parser.
    let expected = match tokens[1] {
        "/stream.flac" | "/stream.flac?cache=1" => Some(
            if active == esp_shazam_core::stream_buffer::OutputFormat::Flac {
                Route::Stream(Format::Flac)
            } else {
                Route::Refuse(Status::NotFound)
            },
        ),
        "/stream.mp3" | "/stream.mp3?cache=1" => Some(
            if active == esp_shazam_core::stream_buffer::OutputFormat::Mp3 {
                Route::Stream(Format::Mp3)
            } else {
                Route::Refuse(Status::NotFound)
            },
        ),
        "/stream.wav" => Some(Route::Stream(Format::Wav)),
        "/missing" => Some(Route::Refuse(Status::NotFound)),
        "/stream.wav?buffer=low" => Some(Route::Refuse(Status::BadRequest)),
        target => {
            let fixtures = [
                (
                    "/stream.flac",
                    esp_shazam_core::stream_buffer::OutputFormat::Flac,
                ),
                (
                    "/stream.mp3",
                    esp_shazam_core::stream_buffer::OutputFormat::Mp3,
                ),
            ];
            fixtures.into_iter().find_map(|(path, format)| {
                [
                    ("buffer=low", Route::Buffered(BufferPreset::Low)),
                    ("buffer=med", Route::Buffered(BufferPreset::Medium)),
                    ("buffer=high", Route::Buffered(BufferPreset::High)),
                    ("buffer=bad", Route::Refuse(Status::BadRequest)),
                    ("buffer=low&buffer=high", Route::Refuse(Status::BadRequest)),
                    ("buffer", Route::Refuse(Status::BadRequest)),
                    ("buffer=", Route::Refuse(Status::BadRequest)),
                    ("buffer=low&cache=1", Route::Refuse(Status::BadRequest)),
                ]
                .into_iter()
                .find_map(|(query, expected)| {
                    // A malformed query is 400 whatever codec is active; a well-formed one on
                    // the inactive path is 404.
                    (target == format!("{path}?{query}")).then_some(
                        if active == format || expected == Route::Refuse(Status::BadRequest) {
                            expected
                        } else {
                            Route::Refuse(Status::NotFound)
                        },
                    )
                })
            })
        }
    };
    if let Some(expected) = expected {
        assert_eq!(route, expected);
    }
    if let Route::Refuse(status) = route {
        let code = match status {
            Status::BadRequest => "400",
            Status::NotFound => "404",
            Status::MethodNotAllowed => "405",
            Status::ServiceUnavailable => "503",
        };
        assert!(status.response().starts_with(&format!("HTTP/1.1 {code} ")));
    }
}

#[cfg(test)]
mod write_contract_tests {
    use super::*;
    use esp_shazam_core::stream_write::{Action, WriteProgress};
    use std::{io, time::Duration};
    #[test]
    fn send_and_poll_events_check_results_progress_and_actions() {
        let now = Duration::ZERO;
        let mut write = WriteProgress::new(4, Duration::from_secs(1));
        checked_sent(&mut write, Ok(1), now).unwrap();
        assert_eq!(write.accepted(), 1);
        checked_sent(&mut write, Err(io::ErrorKind::Interrupted.into()), now).unwrap();
        checked_sent(&mut write, Err(io::ErrorKind::WouldBlock.into()), now).unwrap();
        checked_polled(&mut write, Ok(false), now).unwrap();
        checked_polled(&mut write, Err(io::ErrorKind::Interrupted.into()), now).unwrap();
        checked_polled(&mut write, Ok(true), now).unwrap();
        assert_eq!(write.next(now).unwrap(), Action::Send { offset: 1 });
        checked_sent(&mut write, Ok(3), now).unwrap();
        assert_eq!(write.next(now).unwrap(), Action::Complete);
        for event in [Ok(0), Err(io::ErrorKind::BrokenPipe.into())] {
            let mut write = WriteProgress::new(4, Duration::from_secs(1));
            assert!(checked_sent(&mut write, event, now).is_err());
        }
        let mut write = WriteProgress::new(4, Duration::from_secs(1));
        checked_sent(&mut write, Err(io::ErrorKind::WouldBlock.into()), now).unwrap();
        assert!(checked_polled(&mut write, Err(io::ErrorKind::BrokenPipe.into()), now).is_err());
    }
}

/// Checks only the supplied send event, its result and immediate next action.
pub fn checked_sent(
    write: &mut esp_shazam_core::stream_write::WriteProgress,
    event: std::io::Result<usize>,
    now: std::time::Duration,
) -> std::io::Result<()> {
    use esp_shazam_core::stream_write::Action;
    use std::io::ErrorKind;
    let before = write.accepted();
    let expected = event.as_ref().map(|&n| n).map_err(|e| e.kind());
    let result = write.sent(event);
    let error = match expected {
        Ok(0) => Some(ErrorKind::WriteZero),
        Err(kind) if !matches!(kind, ErrorKind::Interrupted | ErrorKind::WouldBlock) => Some(kind),
        _ => None,
    };
    assert_eq!(result.as_ref().err().map(|e| e.kind()), error);
    assert_eq!(write.accepted(), before + expected.unwrap_or(0));
    if result.is_ok() {
        match write.next(now).unwrap() {
            Action::Complete => assert!(matches!(expected, Ok(n) if n > 0)),
            Action::Poll { timeout_ms } => {
                assert_eq!(expected, Err(ErrorKind::WouldBlock));
                assert!(timeout_ms > 0);
            }
            Action::Send { offset } => {
                assert_ne!(expected, Err(ErrorKind::WouldBlock));
                assert_eq!(offset, write.accepted());
            }
        }
    }
    result
}

/// Checks readiness and interruption without predicting later socket events.
pub fn checked_polled(
    write: &mut esp_shazam_core::stream_write::WriteProgress,
    event: std::io::Result<bool>,
    now: std::time::Duration,
) -> std::io::Result<()> {
    use esp_shazam_core::stream_write::Action;
    use std::io::ErrorKind;
    let before = write.accepted();
    let expected = event.as_ref().copied().map_err(|e| e.kind());
    let result = write.polled(event);
    assert_eq!(
        result.as_ref().err().map(|e| e.kind()),
        expected
            .err()
            .filter(|&kind| kind != ErrorKind::Interrupted)
    );
    assert_eq!(write.accepted(), before);
    if result.is_ok() {
        match write.next(now).unwrap() {
            Action::Send { offset } => {
                assert_eq!(expected, Ok(true));
                assert_eq!(offset, before);
            }
            Action::Poll { timeout_ms } => {
                assert_ne!(expected, Ok(true));
                assert!(timeout_ms > 0);
            }
            Action::Complete => panic!("poll cannot complete an incomplete write"),
        }
    }
    result
}
