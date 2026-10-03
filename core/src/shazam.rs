// ABOUTME: Builds Shazam recognize requests and parses their responses.
// ABOUTME: Mirrors SongRec's communication.rs; callers pass in time, UUIDs and user agent so tests can pin them.

use serde::Serialize;
use serde_json::{json, Value};

const RECOGNIZE_URL: &str = "https://amp.shazam.com/discovery/v5/en/US/android/-/tag/";
const RECOGNIZE_QUERY: &str =
    "?sync=true&webv3=true&sampling=true&connected=&shazamapiversion=v3&sharehub=true&video=v3";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

/// Everything a request needs besides the signature.
pub struct RequestContext<'a> {
    pub unix_ms: u64,
    /// Lower-case v4 UUIDs. SongRec puts the first one in the URL upper-cased.
    pub uuid_a: &'a str,
    pub uuid_b: &'a str,
    pub user_agent: &'a str,
}

pub fn build_request(signature_uri: &str, sample_ms: u32, ctx: &RequestContext) -> Request {
    let timestamp = ctx.unix_ms as u32; // SongRec truncates the same way
    let body = json!({
        "geolocation": {"altitude": 300, "latitude": 45, "longitude": 2},
        "signature": {"samplems": sample_ms, "timestamp": timestamp, "uri": signature_uri},
        "timestamp": timestamp,
        "timezone": "Europe/Paris"
    });
    Request {
        url: format!(
            "{RECOGNIZE_URL}{}/{}{RECOGNIZE_QUERY}",
            ctx.uuid_a.to_uppercase(),
            ctx.uuid_b
        ),
        headers: vec![
            ("Content-Type", "application/json".to_string()),
            ("Content-Language", "en_US".to_string()),
            ("User-Agent", ctx.user_agent.to_string()),
        ],
        body: body.to_string(),
    }
}

/// SongRec's `samplems`: the signature's length in milliseconds.
pub fn sample_ms(number_samples: u32, sample_rate_hz: u32) -> u32 {
    (number_samples as f32 / sample_rate_hz as f32 * 1000.0) as u32
}

/// Formats 16 random bytes as a lower-case version-4 UUID.
pub fn uuid_v4(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// One of SongRec's 100 Android user agents; any index works.
pub fn user_agent(index: usize) -> &'static str {
    let agents = &crate::fingerprinting::user_agent::USER_AGENTS;
    agents[index % agents.len()]
}

/// The song fields esp-shazam reports. Field paths follow SongRec's http_task.rs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Song {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub cover_url: Option<String>,
    pub shazam_url: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    NotJson,
    NotAnObject,
    MissingField(&'static str),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NotJson => write!(f, "response is not JSON"),
            ParseError::NotAnObject => write!(f, "response JSON is not an object"),
            ParseError::MissingField(field) => write!(f, "response track lacks {field}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// `Ok(None)` is a miss: valid JSON object without a `track`.
pub fn parse_response(body: &[u8]) -> Result<Option<Song>, ParseError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ParseError::NotJson)?;
    let object = value.as_object().ok_or(ParseError::NotAnObject)?;
    let Some(track) = object.get("track") else {
        return Ok(None);
    };
    let text = |v: &Value| v.as_str().map(str::to_owned);
    let album = track["sections"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|section| section["type"] == "SONG")
        .flat_map(|section| section["metadata"].as_array().into_iter().flatten())
        .find(|item| item["title"] == "Album")
        .and_then(|item| text(&item["text"]));
    Ok(Some(Song {
        key: text(&track["key"]).ok_or(ParseError::MissingField("track.key"))?,
        title: text(&track["title"]).ok_or(ParseError::MissingField("track.title"))?,
        artist: text(&track["subtitle"]).ok_or(ParseError::MissingField("track.subtitle"))?,
        album,
        cover_url: text(&track["images"]["coverart"]),
        shazam_url: text(&track["url"]),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprinting::user_agent::USER_AGENTS;
    use serde_json::{json, Value};

    #[test]
    fn request_matches_songrec_shape() {
        let ctx = RequestContext {
            unix_ms: 1_791_062_521_123,
            uuid_a: "0f0e0d0c-0b0a-4908-8706-050403020100",
            uuid_b: "00112233-4455-4677-8899-aabbccddeeff",
            user_agent: "Dalvik/2.1.0 (test)",
        };
        let request = build_request("data:audio/vnd.shazam.sig;base64,AAAA", 12_000, &ctx);
        assert_eq!(
            request.url,
            "https://amp.shazam.com/discovery/v5/en/US/android/-/tag/\
             0F0E0D0C-0B0A-4908-8706-050403020100/00112233-4455-4677-8899-aabbccddeeff\
             ?sync=true&webv3=true&sampling=true&connected=&shazamapiversion=v3&sharehub=true&video=v3"
        );
        assert_eq!(
            request.headers,
            vec![
                ("Content-Type", "application/json".to_string()),
                ("Content-Language", "en_US".to_string()),
                ("User-Agent", "Dalvik/2.1.0 (test)".to_string()),
            ]
        );
        // 1_791_062_521_123 truncated to u32, as SongRec does (`timestamp_ms as u32`).
        let ts = 61_158_691u32;
        let body: Value = serde_json::from_str(&request.body).expect("body is JSON");
        assert_eq!(
            body,
            json!({
                "geolocation": {"altitude": 300, "latitude": 45, "longitude": 2},
                "signature": {"samplems": 12_000, "timestamp": ts, "uri": "data:audio/vnd.shazam.sig;base64,AAAA"},
                "timestamp": ts,
                "timezone": "Europe/Paris"
            })
        );
    }

    #[test]
    fn sample_ms_is_signature_length() {
        assert_eq!(sample_ms(192_000, 16_000), 12_000);
        assert_eq!(sample_ms(100_000, 16_000), 6_250);
    }

    #[test]
    fn uuid_v4_sets_version_and_variant_bits() {
        assert_eq!(uuid_v4([0; 16]), "00000000-0000-4000-8000-000000000000");
        assert_eq!(uuid_v4([0xff; 16]), "ffffffff-ffff-4fff-bfff-ffffffffffff");
    }

    #[test]
    fn user_agent_wraps_around_songrecs_list() {
        assert_eq!(user_agent(0), USER_AGENTS[0]);
        assert_eq!(user_agent(100), USER_AGENTS[0]);
        assert_eq!(user_agent(199), USER_AGENTS[99]);
    }

    #[test]
    fn html_error_page_is_not_json() {
        assert_eq!(
            parse_response(b"<html><body>502 Bad Gateway</body></html>"),
            Err(ParseError::NotJson)
        );
    }

    #[test]
    fn empty_body_is_not_json() {
        assert_eq!(parse_response(b""), Err(ParseError::NotJson));
    }

    #[test]
    fn json_that_is_not_an_object_is_rejected() {
        assert_eq!(parse_response(b"[]"), Err(ParseError::NotAnObject));
    }

    #[test]
    fn object_without_track_is_a_miss() {
        assert_eq!(
            parse_response(br#"{"matches": [], "retryms": 12000}"#),
            Ok(None)
        );
    }

    #[test]
    fn track_without_title_is_rejected() {
        let body = br#"{"track": {"key": "1", "subtitle": "Artist"}}"#;
        assert_eq!(
            parse_response(body),
            Err(ParseError::MissingField("track.title"))
        );
    }

    #[test]
    fn minimal_non_ascii_track_parses() {
        let body = r#"{"track": {"key": "42", "title": "Beyoncé", "subtitle": "夜に駆ける"}}"#;
        assert_eq!(
            parse_response(body.as_bytes()),
            Ok(Some(Song {
                key: "42".into(),
                title: "Beyoncé".into(),
                artist: "夜に駆ける".into(),
                album: None,
                cover_url: None,
                shazam_url: None,
            }))
        );
    }

    #[test]
    fn album_cover_and_link_come_from_their_paths() {
        let body = r#"{"track": {"key": "1", "title": "T", "subtitle": "A",
            "url": "https://www.shazam.com/track/1/t",
            "images": {"coverart": "https://is1-ssl.mzstatic.com/x.jpg"},
            "sections": [{"type": "LYRICS"},
                         {"type": "SONG", "metadata": [{"title": "Label", "text": "L"},
                                                       {"title": "Album", "text": "The Album"}]}]}}"#;
        let song = parse_response(body.as_bytes()).unwrap().unwrap();
        assert_eq!(song.album.as_deref(), Some("The Album"));
        assert_eq!(
            song.cover_url.as_deref(),
            Some("https://is1-ssl.mzstatic.com/x.jpg")
        );
        assert_eq!(
            song.shazam_url.as_deref(),
            Some("https://www.shazam.com/track/1/t")
        );
    }
}
