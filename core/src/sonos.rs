// ABOUTME: Bounded Sonos SSDP discovery, ID validation and the saved controller config for the firmware adapter.
// ABOUTME: Pure autoplay decisions keep clocks and session ownership out of network code.

use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

pub const MAX_DISCOVERY_BYTES: usize = 4096;
const SERVICE: &str = "urn:smartspeaker-audio:service:SpeakerGroup:1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Malformed,
    InvalidEndpoint,
    InvalidConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryGroup {
    pub player_id: String,
    pub group_id: String,
    pub household_id: String,
    pub name: String,
    pub host: Ipv4Addr,
    pub is_leader: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SonosConfig {
    /// Sonos control is opt-in: off until the settings page turns it on. A saved config without the field is off.
    #[serde(default)]
    pub enabled: bool,
    pub player_id: Option<String>,
    pub autoplay: bool,
    #[serde(default)]
    pub buffer_preset: crate::stream_buffer::BufferPreset,
    #[serde(default)]
    pub output_format: crate::stream_buffer::OutputFormat,
}

impl SonosConfig {
    /// Automatic playback cannot run while Sonos control is off, however the saved flag reads
    /// (a config saved before the opt-in may carry `autoplay: true`). Every reader of the flag asks here.
    pub fn autoplay(&self) -> bool {
        self.autoplay && self.enabled
    }
}

/// The loaded config as the controller must act on it, so a stale `autoplay` cannot wake up when control is turned on.
pub fn effective(config: SonosConfig) -> SonosConfig {
    SonosConfig {
        autoplay: config.autoplay(),
        ..config
    }
}

/// Parse one SSDP response, retaining followers for stable player-to-group resolution.
/// The adapter bounds count/lifetime, resolves the leader, and filters leaders for the UI.
pub fn parse_advertisement(packet: &[u8], peer: Ipv4Addr) -> Result<Option<DiscoveryGroup>, Error> {
    if packet.len() > MAX_DISCOVERY_BYTES {
        return Err(Error::TooLarge);
    }
    let text = std::str::from_utf8(packet).map_err(|_| Error::Malformed)?;
    let text = text.strip_suffix("\r\n\r\n").ok_or(Error::Malformed)?;
    let mut lines = text.split("\r\n");
    if lines.next() != Some("HTTP/1.1 200 OK") {
        return Err(Error::Malformed);
    }
    let mut headers: Vec<(&str, &str)> = Vec::new();
    for line in lines {
        let (key, value) = line.split_once(':').ok_or(Error::Malformed)?;
        let value = value.trim_matches([' ', '\t']);
        if headers.len() >= 32 || key.len() > 64 || value.len() > 512 {
            return Err(Error::TooLarge);
        }
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            || value.chars().any(char::is_control)
            || headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(key))
        {
            return Err(Error::Malformed);
        }
        headers.push((key, value));
    }
    let get = |key: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
            .ok_or(Error::Malformed)
    };
    if get("st")? != SERVICE {
        return Ok(None);
    }
    let info = get("groupinfo.smartspeaker.audio")?;
    // Semicolons inside the quoted display name are not field separators.
    let mut quoted = false;
    let mut escaped = false;
    let mut fields = Vec::new();
    let mut start = 0;
    for (i, c) in info.char_indices() {
        if escaped {
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == ';' && !quoted {
            fields.push(&info[start..i]);
            start = i + 1;
        }
    }
    if quoted || escaped {
        return Err(Error::Malformed);
    }
    fields.push(&info[start..]);
    let mut leader = None;
    let mut group = None;
    let mut name = None;
    for field in fields {
        let (k, v) = field.trim().split_once('=').ok_or(Error::Malformed)?;
        let slot = match k.trim() {
            "gc" => &mut leader,
            "gid" => &mut group,
            "gname" => &mut name,
            _ => return Err(Error::Malformed),
        };
        if slot.replace(v.trim()).is_some() {
            return Err(Error::Malformed);
        }
    }
    let is_leader = match leader {
        Some("0") => false,
        Some("1") => true,
        _ => return Err(Error::Malformed),
    };
    if get("websock.smartspeaker.audio")? != format!("wss://{peer}:1443/websocket/api") {
        return Err(Error::InvalidEndpoint);
    }
    let usn = get("usn")?.strip_prefix("uuid:").ok_or(Error::Malformed)?;
    let (player_id, service) = usn.split_once("::").ok_or(Error::Malformed)?;
    if service != SERVICE {
        return Err(Error::Malformed);
    }
    let group_id = group.ok_or(Error::Malformed)?;
    let household_id = get("household.smartspeaker.audio")?;
    for id in [player_id, group_id, household_id] {
        validate_id(id)?;
    }
    let name: String =
        serde_json::from_str(name.ok_or(Error::Malformed)?).map_err(|_| Error::Malformed)?;
    if name.len() > 256 || name.chars().any(char::is_control) {
        return Err(Error::Malformed);
    }
    Ok(Some(DiscoveryGroup {
        player_id: player_id.into(),
        group_id: group_id.into(),
        household_id: household_id.into(),
        name,
        host: peer,
        is_leader,
    }))
}

pub fn validate_id(id: &str) -> Result<(), Error> {
    if id.is_empty() || id.len() > 256 || !id.bytes().all(|b| b.is_ascii_graphic()) {
        Err(Error::Malformed)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Start,
    Stop,
}

#[derive(Debug, Clone, Copy)]
pub enum AutoplayEvent {
    Level(f32),
    Enabled(bool),
    /// Only report confirmation for the adapter's current, still-wanted session.
    OwnershipAcquired,
    /// Includes eviction, failed start and uncertain ownership after disconnect.
    OwnershipLost,
    ManualStop,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    WaitingQuiet,
    Armed,
    Starting,
    Owned,
}

/// Dwell times use monotonic milliseconds supplied by one controller worker.
/// Thresholds need calibration against real line-in noise; this policy has no guessed defaults.
pub struct Autoplay {
    active_dbfs: f32,
    quiet_dbfs: f32,
    enabled: bool,
    phase: Phase,
    last_ms: Option<u64>,
    active_since: Option<u64>,
    quiet_since: Option<u64>,
}

impl Autoplay {
    pub fn new(active_dbfs: f32, quiet_dbfs: f32) -> Result<Self, Error> {
        if !active_dbfs.is_finite() || !quiet_dbfs.is_finite() || active_dbfs <= quiet_dbfs {
            return Err(Error::InvalidConfig);
        }
        Ok(Self {
            active_dbfs,
            quiet_dbfs,
            enabled: false,
            phase: Phase::WaitingQuiet,
            last_ms: None,
            active_since: None,
            quiet_since: None,
        })
    }

    /// Start is issued once, then awaits OwnershipAcquired or OwnershipLost.
    /// Disabling detection preserves an owned session; manual Stop still releases it.
    /// Ownership loss never emits Stop. The adapter cancels pending I/O on Stop/loss/disable.
    pub fn update(&mut self, now_ms: u64, event: AutoplayEvent) -> Action {
        let regressed = self.last_ms.is_some_and(|last| now_ms < last);
        self.last_ms = Some(self.last_ms.unwrap_or(now_ms).max(now_ms));
        if regressed {
            self.invalidate_detection();
        }
        match event {
            AutoplayEvent::Enabled(enabled) => {
                if self.enabled != enabled {
                    self.enabled = enabled;
                    if self.phase == Phase::Starting {
                        self.phase = Phase::WaitingQuiet;
                    }
                    self.invalidate_detection();
                }
            }
            AutoplayEvent::OwnershipAcquired => {
                self.phase = Phase::Owned;
                self.active_since = None;
                self.quiet_since = None;
            }
            AutoplayEvent::OwnershipLost | AutoplayEvent::ManualStop => {
                let stop = matches!(event, AutoplayEvent::ManualStop) && self.phase == Phase::Owned;
                self.phase = Phase::WaitingQuiet;
                self.invalidate_detection();
                if stop {
                    return Action::Stop;
                }
            }
            AutoplayEvent::Level(level) => {
                if regressed || !level.is_finite() {
                    self.invalidate_detection();
                    return Action::None;
                }
                if !self.enabled {
                    return Action::None;
                }
                if level < self.quiet_dbfs {
                    self.active_since = None;
                    let since = *self.quiet_since.get_or_insert(now_ms);
                    if now_ms - since >= 10_000 {
                        match self.phase {
                            Phase::WaitingQuiet => self.phase = Phase::Armed,
                            Phase::Owned => {
                                self.phase = Phase::WaitingQuiet;
                                self.invalidate_detection();
                                return Action::Stop;
                            }
                            _ => (),
                        }
                    }
                } else {
                    self.quiet_since = None;
                    if level > self.active_dbfs && self.phase == Phase::Armed {
                        let since = *self.active_since.get_or_insert(now_ms);
                        if now_ms - since >= 500 {
                            self.phase = Phase::Starting;
                            self.active_since = None;
                            return Action::Start;
                        }
                    } else {
                        self.active_since = None;
                    }
                }
            }
        }
        Action::None
    }

    fn invalidate_detection(&mut self) {
        self.active_since = None;
        self.quiet_since = None;
        if self.phase == Phase::Armed {
            self.phase = Phase::WaitingQuiet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEER: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 20);

    fn discovery() -> String {
        format!("HTTP/1.1 200 OK\r\nST: {SERVICE}\r\nUSN: uuid:RINCON_PLAYER::{}\r\nGROUPINFO.SMARTSPEAKER.AUDIO: gc=1; gid=RINCON_GROUP:123; gname=\"Room + 2\"\r\nHOUSEHOLD.SMARTSPEAKER.AUDIO: Sonos_EXAMPLE\r\nWEBSOCK.SMARTSPEAKER.AUDIO: wss://192.0.2.20:1443/websocket/api\r\n\r\n", SERVICE)
    }

    #[test]
    fn discovery_keeps_group_suffix_and_usn_player_identity() {
        let g = parse_advertisement(discovery().as_bytes(), PEER)
            .unwrap()
            .unwrap();
        assert_eq!(g.group_id, "RINCON_GROUP:123");
        assert_eq!(g.player_id, "RINCON_PLAYER");
        assert_eq!(g.household_id, "Sonos_EXAMPLE");
        assert_eq!(g.name, "Room + 2");
        assert_eq!(g.host, PEER);
        assert!(g.is_leader);
        let lowercase = discovery().replace("ST:", "st:").replace("USN:", "usn:");
        assert_eq!(
            parse_advertisement(lowercase.as_bytes(), PEER).unwrap(),
            Some(g)
        );
    }

    #[test]
    fn discovery_retains_followers_for_stable_player_resolution() {
        let packet = discovery().replace("gc=1", "gc=0").replace("Room + 2", "");
        let follower = parse_advertisement(packet.as_bytes(), PEER)
            .unwrap()
            .unwrap();
        assert_eq!(follower.player_id, "RINCON_PLAYER");
        assert_eq!(follower.group_id, "RINCON_GROUP:123");
        assert_eq!(follower.name, "");
        assert!(!follower.is_leader);
        assert_eq!(
            parse_advertisement(
                packet
                    .replace("wss://192.0.2.20", "wss://192.0.2.21")
                    .as_bytes(),
                PEER
            ),
            Err(Error::InvalidEndpoint)
        );
    }

    #[test]
    fn discovery_skips_other_services() {
        assert_eq!(
            parse_advertisement(
                discovery().replace(SERVICE, "other-service").as_bytes(),
                PEER
            ),
            Ok(None)
        );
    }

    #[test]
    fn discovery_endpoint_must_be_exact_peer_port_and_path() {
        for url in [
            "ws://192.0.2.20:1443/websocket/api",
            "wss://192.0.2.21:1443/websocket/api",
            "wss://192.0.2.20:443/websocket/api",
            "wss://192.0.2.20:1443/websocket/api?x",
            "wss://user@192.0.2.20:1443/websocket/api",
            "wss://192.0.2.20:1443/websocket/api/",
        ] {
            assert_eq!(
                parse_advertisement(
                    discovery()
                        .replace("wss://192.0.2.20:1443/websocket/api", url)
                        .as_bytes(),
                    PEER
                ),
                Err(Error::InvalidEndpoint)
            );
        }
    }

    #[test]
    fn discovery_rejects_malformed_duplicate_and_unbounded_fields() {
        for packet in [
            discovery().replace("gc=1", "gc=2"),
            discovery().replace("uuid:", ""),
            discovery().replace("\r\n\r\n", "\r\nst: duplicate\r\n\r\n"),
            discovery().replace("Sonos_EXAMPLE", &"x".repeat(257)),
            discovery().replace("\r\n\r\n", "\r\n"),
            discovery().replace("gc=1;", "gc=1; gc=0;"),
            discovery().replace("Room + 2", "bad\nname"),
        ] {
            assert!(
                parse_advertisement(packet.as_bytes(), PEER).is_err(),
                "accepted malformed packet"
            );
        }
        assert_eq!(
            parse_advertisement(&vec![b'x'; 4097], PEER),
            Err(Error::TooLarge)
        );
        assert!(parse_advertisement(&[0xff], PEER).is_err());
        let many = discovery().replace(
            "\r\n\r\n",
            &format!(
                "\r\n{}\r\n",
                (0..33).map(|i| format!("X-{i}: a\r\n")).collect::<String>()
            ),
        );
        assert!(parse_advertisement(many.as_bytes(), PEER).is_err());
    }

    #[test]
    fn discovery_accepts_semicolon_in_quoted_group_name() {
        let packet = discovery().replace("Room + 2", "Room; Two");
        assert_eq!(
            parse_advertisement(packet.as_bytes(), PEER)
                .unwrap()
                .unwrap()
                .name,
            "Room; Two"
        );
    }

    #[test]
    fn settings_default_to_no_destination_or_autoplay() {
        let saved_before_opt_in = SonosConfig {
            autoplay: true,
            ..SonosConfig::default()
        };
        assert!(
            !saved_before_opt_in.autoplay(),
            "autoplay is inert while Sonos is off"
        );
        assert!(
            !effective(saved_before_opt_in.clone()).autoplay,
            "the loaded config carries the same rule"
        );
        let opted_in = SonosConfig {
            enabled: true,
            ..saved_before_opt_in
        };
        assert!(opted_in.autoplay());
        assert!(effective(opted_in).autoplay);
        assert_eq!(
            SonosConfig::default(),
            SonosConfig {
                enabled: false,
                player_id: None,
                autoplay: false,
                buffer_preset: crate::stream_buffer::BufferPreset::Medium,
                output_format: crate::stream_buffer::OutputFormat::Flac,
            }
        );
    }

    #[test]
    fn saved_settings_default_and_round_trip_buffer_presets() {
        let old = r#"{"player_id":"player","autoplay":true}"#;
        let mut config: SonosConfig = serde_json::from_str(old).unwrap();
        assert_eq!(
            serde_json::to_value(&config).unwrap()["buffer_preset"],
            "med"
        );
        for value in ["low", "med", "high"] {
            let json =
                format!(r#"{{"player_id":"player","autoplay":true,"buffer_preset":"{value}"}}"#);
            config = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_value(&config).unwrap()["buffer_preset"],
                value
            );
            assert_eq!(config.player_id.as_deref(), Some("player"));
            assert!(config.autoplay);
        }
        assert!(serde_json::from_str::<SonosConfig>(
            r#"{"player_id":null,"autoplay":false,"buffer_preset":"4"}"#
        )
        .is_err());
    }

    fn autoplay() -> Autoplay {
        let mut p = Autoplay::new(-40.0, -50.0).unwrap();
        assert_eq!(p.update(0, AutoplayEvent::Enabled(true)), Action::None);
        p
    }

    fn armed() -> Autoplay {
        let mut p = autoplay();
        assert_eq!(p.update(0, AutoplayEvent::Level(-60.0)), Action::None);
        assert_eq!(p.update(10_000, AutoplayEvent::Level(-60.0)), Action::None);
        p
    }

    #[test]
    fn autoplay_requires_quiet_at_boot_then_500ms_of_new_audio() {
        let mut p = autoplay();
        assert_eq!(p.update(0, AutoplayEvent::Level(-20.0)), Action::None);
        assert_eq!(p.update(60_000, AutoplayEvent::Level(-20.0)), Action::None);
        assert_eq!(p.update(60_001, AutoplayEvent::Level(-60.0)), Action::None);
        assert_eq!(p.update(70_000, AutoplayEvent::Level(-60.0)), Action::None);
        assert_eq!(p.update(70_001, AutoplayEvent::Level(-60.0)), Action::None);
        assert_eq!(p.update(70_002, AutoplayEvent::Level(-20.0)), Action::None);
        assert_eq!(p.update(70_501, AutoplayEvent::Level(-20.0)), Action::None);
        assert_eq!(p.update(70_502, AutoplayEvent::Level(-20.0)), Action::Start);
        assert_eq!(p.update(90_000, AutoplayEvent::Level(-20.0)), Action::None);
    }

    #[test]
    fn autoplay_quiet_stop_requires_owned_session_and_ten_continuous_seconds() {
        let mut p = armed();
        p.update(11_000, AutoplayEvent::Level(-20.0));
        assert_eq!(p.update(11_500, AutoplayEvent::Level(-20.0)), Action::Start);
        p.update(12_000, AutoplayEvent::Level(-60.0));
        assert_eq!(p.update(22_000, AutoplayEvent::Level(-60.0)), Action::None);
        assert_eq!(
            p.update(22_001, AutoplayEvent::OwnershipAcquired),
            Action::None
        );
        p.update(23_000, AutoplayEvent::Level(-60.0));
        assert_eq!(p.update(32_999, AutoplayEvent::Level(-60.0)), Action::None);
        assert_eq!(p.update(33_000, AutoplayEvent::Level(-60.0)), Action::Stop);
        assert_eq!(p.update(34_000, AutoplayEvent::Level(-60.0)), Action::None);
        assert_eq!(p.update(34_001, AutoplayEvent::Level(-20.0)), Action::None);
        assert_eq!(p.update(34_501, AutoplayEvent::Level(-20.0)), Action::None);
    }

    #[test]
    fn autoplay_hysteresis_band_interrupts_dwell_without_triggering() {
        let mut p = armed();
        p.update(11_000, AutoplayEvent::Level(-20.0));
        p.update(11_499, AutoplayEvent::Level(-40.0));
        assert_eq!(p.update(11_500, AutoplayEvent::Level(-20.0)), Action::None);
        assert_eq!(p.update(12_000, AutoplayEvent::Level(-20.0)), Action::Start);
        p.update(12_001, AutoplayEvent::OwnershipAcquired);
        p.update(13_000, AutoplayEvent::Level(-60.0));
        p.update(22_999, AutoplayEvent::Level(-50.0));
        assert_eq!(p.update(23_000, AutoplayEvent::Level(-60.0)), Action::None);
        assert_eq!(p.update(33_000, AutoplayEvent::Level(-60.0)), Action::Stop);
    }

    #[test]
    fn autoplay_stop_eviction_and_failed_start_require_fresh_quiet() {
        for (event, owned, expected) in [
            (AutoplayEvent::ManualStop, true, Action::Stop),
            (AutoplayEvent::OwnershipLost, true, Action::None),
            (AutoplayEvent::OwnershipLost, false, Action::None),
            (AutoplayEvent::ManualStop, false, Action::None),
        ] {
            let mut p = armed();
            p.update(11_000, AutoplayEvent::Level(-20.0));
            assert_eq!(p.update(11_500, AutoplayEvent::Level(-20.0)), Action::Start);
            if owned {
                p.update(11_501, AutoplayEvent::OwnershipAcquired);
            }
            assert_eq!(p.update(12_000, event), expected);
            assert_eq!(p.update(13_000, AutoplayEvent::Level(-20.0)), Action::None);
            assert_eq!(p.update(99_000, AutoplayEvent::Level(-20.0)), Action::None);
            p.update(100_000, AutoplayEvent::Level(-60.0));
            p.update(110_000, AutoplayEvent::Level(-60.0));
            p.update(110_001, AutoplayEvent::Level(-20.0));
            assert_eq!(
                p.update(110_501, AutoplayEvent::Level(-20.0)),
                Action::Start
            );
        }
    }

    #[test]
    fn autoplay_disabled_or_reenabled_does_not_start_during_music() {
        let mut p = Autoplay::new(-40.0, -50.0).unwrap();
        for (time, level) in [
            (0, -60.0),
            (10_000, -60.0),
            (11_000, -20.0),
            (12_000, -20.0),
        ] {
            assert_eq!(p.update(time, AutoplayEvent::Level(level)), Action::None);
        }
        let mut p = armed();
        p.update(11_000, AutoplayEvent::Level(-20.0));
        p.update(11_250, AutoplayEvent::Enabled(false));
        p.update(11_500, AutoplayEvent::Enabled(true));
        assert_eq!(p.update(12_000, AutoplayEvent::Level(-20.0)), Action::None);
        assert_eq!(p.update(13_000, AutoplayEvent::Level(-20.0)), Action::None);
    }

    #[test]
    fn autoplay_nonfinite_levels_and_clock_regression_cannot_finish_dwell() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut p = armed();
            p.update(11_000, AutoplayEvent::Level(-20.0));
            assert_eq!(p.update(11_250, AutoplayEvent::Level(bad)), Action::None);
            assert_eq!(p.update(11_500, AutoplayEvent::Level(-20.0)), Action::None);
            assert_eq!(p.update(12_000, AutoplayEvent::Level(-20.0)), Action::None);
        }
        let mut p = armed();
        p.update(11_000, AutoplayEvent::Level(-20.0));
        assert_eq!(p.update(10_000, AutoplayEvent::Level(-20.0)), Action::None);
        assert_eq!(p.update(11_500, AutoplayEvent::Level(-20.0)), Action::None);
        p.update(12_000, AutoplayEvent::Level(-60.0));
        p.update(22_000, AutoplayEvent::Level(-60.0));
        p.update(22_001, AutoplayEvent::Level(-20.0));
        assert_eq!(p.update(22_501, AutoplayEvent::Level(-20.0)), Action::Start);
        p.update(22_502, AutoplayEvent::OwnershipAcquired);
        assert_eq!(p.update(1, AutoplayEvent::OwnershipLost), Action::None);
        assert_eq!(p.update(23_000, AutoplayEvent::ManualStop), Action::None);
    }

    #[test]
    fn autoplay_validates_threshold_order_and_finiteness() {
        for (active, quiet) in [
            (-50.0, -40.0),
            (-40.0, -40.0),
            (f32::NAN, -50.0),
            (-40.0, f32::NEG_INFINITY),
        ] {
            assert!(matches!(
                Autoplay::new(active, quiet),
                Err(Error::InvalidConfig)
            ));
        }
    }

    #[test]
    fn invalid_observation_does_not_allow_second_start_while_first_is_pending() {
        for invalid_time in [11_750, 1] {
            let mut p = armed();
            p.update(11_000, AutoplayEvent::Level(-20.0));
            assert_eq!(p.update(11_500, AutoplayEvent::Level(-20.0)), Action::Start);
            p.update(invalid_time, AutoplayEvent::Level(f32::NAN));
            p.update(12_000, AutoplayEvent::Level(-60.0));
            p.update(22_000, AutoplayEvent::Level(-60.0));
            p.update(23_000, AutoplayEvent::Level(-20.0));
            assert_eq!(p.update(23_500, AutoplayEvent::Level(-20.0)), Action::None);
        }
    }

    #[test]
    fn discovery_does_not_trim_away_invalid_line_breaks() {
        let packet = discovery().replace("Sonos_EXAMPLE\r\n", "Sonos_EXAMPLE\n\r\n");
        assert_eq!(
            parse_advertisement(packet.as_bytes(), PEER),
            Err(Error::Malformed)
        );
    }
}
