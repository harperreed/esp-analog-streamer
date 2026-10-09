// ABOUTME: Bounded web commands for local Sonos playback and destination settings.
// ABOUTME: Parses actions without accepting arbitrary URLs or unknown fields.

pub const MAX_BODY_BYTES: usize = 512;

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Discover {},
    Play {},
    Stop {},
    Select {
        player_id: String,
    },
    Autoplay {
        enabled: bool,
    },
    Buffer {
        preset: crate::stream_buffer::BufferPreset,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Invalid,
}

pub fn parse(body: &[u8]) -> Result<Action, Error> {
    if body.len() > MAX_BODY_BYTES {
        return Err(Error::TooLarge);
    }
    let action: Action = serde_json::from_slice(body).map_err(|_| Error::Invalid)?;
    if let Action::Select { player_id } = &action {
        super::sonos::validate_id(player_id).map_err(|_| Error::Invalid)?;
    }
    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_named_buffer_presets() {
        for preset in ["low", "med", "high"] {
            let body = format!(r#"{{"action":"buffer","preset":"{preset}"}}"#);
            assert!(parse(body.as_bytes()).is_ok());
        }
        for body in [
            r#"{"action":"buffer","preset":"medium"}"#,
            r#"{"action":"buffer","preset":4}"#,
            r#"{"action":"buffer","preset":"low","url":"http://evil"}"#,
            r#"{"action":"buffer"}"#,
        ] {
            assert!(parse(body.as_bytes()).is_err());
        }
    }

    #[test]
    fn accepts_only_supported_actions_and_fields() {
        assert_eq!(parse(br#"{"action":"play"}"#), Ok(Action::Play {}));
        assert_eq!(parse(br#"{"action":"stop"}"#), Ok(Action::Stop {}));
        assert_eq!(parse(br#"{"action":"discover"}"#), Ok(Action::Discover {}));
        assert_eq!(
            parse(br#"{"action":"autoplay","enabled":true}"#),
            Ok(Action::Autoplay { enabled: true })
        );
        assert_eq!(
            parse(br#"{"action":"select","player_id":"RINCON_TEST01400"}"#),
            Ok(Action::Select {
                player_id: "RINCON_TEST01400".into()
            })
        );
        for body in [
            br#"{"action":"play","url":"http://example.com"}"#.as_slice(),
            br#"{"action":"autoplay","enabled":"true"}"#,
            br#"{"action":"select","player_id":""}"#,
            br#"{"action":"select","player_id":"bad\nidentity"}"#,
            br#"{"action":"unknown"}"#,
            br#"{"action":"play","action":"stop"}"#,
        ] {
            assert!(parse(body).is_err(), "accepted {:?}", body);
        }
    }

    #[test]
    fn accepts_the_same_identity_contract_as_discovery() {
        let id = "x".repeat(256);
        let body = serde_json::json!({"action":"select","player_id":id}).to_string();
        assert!(parse(body.as_bytes()).is_ok());
        assert!(parse(br#"{"action":"select","player_id":"RINCON.test+id"}"#).is_ok());
    }

    #[test]
    fn rejects_oversized_and_incomplete_requests() {
        assert!(parse(&vec![b' '; MAX_BODY_BYTES + 1]).is_err());
        assert!(parse(b"{").is_err());
        let body = format!(r#"{{"action":"select","player_id":"{}"}}"#, "x".repeat(257));
        assert!(parse(body.as_bytes()).is_err());
    }
}
