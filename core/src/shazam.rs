// ABOUTME: Builds Shazam recognize requests and parses their responses.
// ABOUTME: Mirrors SongRec's communication.rs; callers pass in time, UUIDs and user agent so tests can pin them.

use serde_json::json;

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
}
