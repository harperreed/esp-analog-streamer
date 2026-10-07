// ABOUTME: Bounded Sonos discovery and control messages for the firmware adapter.
// ABOUTME: Pure autoplay decisions keep clocks and session ownership out of network code.

use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

pub const MAX_DISCOVERY_BYTES: usize = 4096;
pub const MAX_MESSAGE_BYTES: usize = 8192;
pub const MAX_MESSAGE_TOKENS: usize = 256;
const SERVICE: &str = "urn:smartspeaker-audio:service:SpeakerGroup:1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Malformed,
    InvalidEndpoint,
    InvalidConfig,
    InvalidFragment,
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
    pub player_id: Option<String>,
    pub autoplay: bool,
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

/// Scope identifiers come from validated discovery or an observed session.
#[derive(Debug, Serialize)]
pub enum Scope<'a> {
    #[serde(rename = "playerId")]
    Player(&'a str),
    #[serde(rename = "groupId")]
    Group(&'a str),
    #[serde(rename = "sessionId")]
    Session(&'a str),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Command<'a> {
    pub namespace: &'a str,
    pub command: &'a str,
    pub household_id: &'a str,
    pub cmd_id: &'a str,
    #[serde(flatten)]
    pub scope: Scope<'a>,
}

/// Serialize through a capped writer; oversized payloads never allocate a full JSON copy.
pub fn encode_command(command: &Command<'_>, body: &serde_json::Value) -> Result<Vec<u8>, Error> {
    for id in [
        command.namespace,
        command.command,
        command.household_id,
        command.cmd_id,
        match command.scope {
            Scope::Player(id) | Scope::Group(id) | Scope::Session(id) => id,
        },
    ] {
        validate_id(id)?;
    }
    if !body.is_object() {
        return Err(Error::Malformed);
    }
    fn check_depth(value: &serde_json::Value, depth: usize) -> Result<(), Error> {
        if depth > 16 {
            return Err(Error::TooLarge);
        }
        match value {
            serde_json::Value::Array(a) => {
                for v in a {
                    check_depth(v, depth + 1)?;
                }
            }
            serde_json::Value::Object(o) => {
                for v in o.values() {
                    check_depth(v, depth + 1)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    check_depth(body, 2)?;
    struct Writer(Vec<u8>);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_MESSAGE_BYTES - self.0.len() {
                return Err(std::io::Error::other("Sonos message exceeds limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(Vec::with_capacity(MAX_MESSAGE_BYTES));
    serde_json::to_writer(&mut writer, &(command, body)).map_err(|_| Error::TooLarge)?;
    Ok(writer.0)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageHeader {
    pub namespace: String,
    pub name: Option<String>,
    pub household_id: Option<String>,
    pub group_id: Option<String>,
    pub session_id: Option<String>,
    pub player_id: Option<String>,
    pub location_id: Option<String>,
    pub response: Option<String>,
    pub success: Option<bool>,
    pub cmd_id: Option<String>,
    #[serde(rename = "type")]
    pub message_type: Option<String>,
}

#[derive(Debug)]
pub struct Message {
    pub header: MessageHeader,
    pub body: serde_json::Value,
}

pub fn parse_message(bytes: &[u8]) -> Result<Message, Error> {
    // Cap wire bytes, nesting and values/keys before serde allocates collection nodes.
    // A short array of tiny containers can otherwise expand far beyond its wire size.
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(Error::TooLarge);
    }
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    let (mut tokens, mut literal) = (0usize, false);
    for &b in bytes {
        if escaped {
            escaped = false;
        } else if quoted && b == b'\\' {
            escaped = true;
        } else if b == b'"' {
            if !quoted {
                tokens += 1; // Both object keys and string values count.
            }
            quoted = !quoted;
            literal = false;
        } else if !quoted {
            match b {
                b'[' | b'{' => {
                    tokens += 1; // Count the container itself, including empty ones.
                    literal = false;
                    depth += 1;
                    if depth > 16 {
                        return Err(Error::TooLarge);
                    }
                }
                b']' | b'}' => {
                    literal = false;
                    depth = depth.checked_sub(1).ok_or(Error::Malformed)?;
                }
                b',' | b':' | b' ' | b'\t' | b'\r' | b'\n' => literal = false,
                _ if !literal => {
                    tokens += 1; // One number, boolean or null; serde checks its syntax.
                    literal = true;
                }
                _ => (),
            }
        }
        if tokens > MAX_MESSAGE_TOKENS {
            return Err(Error::TooLarge);
        }
    }
    let (header, body): (MessageHeader, serde_json::Value) =
        serde_json::from_slice(bytes).map_err(|_| Error::Malformed)?;
    validate_id(&header.namespace)?;
    for id in [
        &header.cmd_id,
        &header.response,
        &header.message_type,
        &header.name,
        &header.household_id,
        &header.group_id,
        &header.session_id,
        &header.player_id,
        &header.location_id,
    ]
    .into_iter()
    .flatten()
    {
        validate_id(id)?;
    }
    if !body.is_object() {
        return Err(Error::Malformed);
    }
    // A reply needs all three markers; events cannot masquerade as acknowledgements.
    let markers = [
        header.cmd_id.is_some(),
        header.response.is_some(),
        header.success.is_some(),
    ];
    if markers.iter().any(|v| *v) && !markers.iter().all(|v| *v) {
        return Err(Error::Malformed);
    }
    Ok(Message { header, body })
}

/// A single in-flight command. The adapter discards it on expiry or disconnect.
pub struct PendingCommand<'a> {
    pub cmd_id: &'a str,
    pub namespace: &'a str,
    pub command: &'a str,
    pub sent_at_ms: u64,
    pub deadline_ms: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReplyMatch {
    Unrelated,
    Success,
    Failure,
    Expired,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub session_id: String,
    pub session_state: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionError {
    pub error_code: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackStatus {
    pub playback_state: String,
    pub item_id: Option<String>,
}

/// The session ID belongs to MessageHeader, as in the speaker's sessionInfo event.
#[derive(Debug, PartialEq, Eq, Deserialize)]
pub struct SessionInfo {
    pub suspended: bool,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackError {
    pub error_code: String,
    pub reason: Option<String>,
    pub item_id: Option<String>,
    pub service_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    SessionStatus(SessionStatus),
    SessionError(SessionError),
    SessionInfo(SessionInfo),
    PlaybackStatus(PlaybackStatus),
    PlaybackError(PlaybackError),
}

impl Message {
    pub fn match_reply(&self, pending: &PendingCommand<'_>, now_ms: u64) -> ReplyMatch {
        if now_ms < pending.sent_at_ms || now_ms >= pending.deadline_ms {
            return ReplyMatch::Expired;
        }
        if self.header.cmd_id.as_deref() != Some(pending.cmd_id)
            || self.header.namespace != pending.namespace
            || self.header.response.as_deref() != Some(pending.command)
        {
            return ReplyMatch::Unrelated;
        }
        match self.header.success {
            Some(true) => ReplyMatch::Success,
            Some(false) => ReplyMatch::Failure,
            None => ReplyMatch::Unrelated,
        }
    }

    /// Unknown events remain available as a bounded body for adapter-specific handling.
    /// The adapter must match the session ID before applying an ownership loss.
    pub fn event(&self) -> Result<Option<Event>, Error> {
        if self.header.cmd_id.is_some() || self.header.response.is_some() {
            return Ok(None);
        }
        let namespace = self
            .header
            .namespace
            .strip_suffix(":1")
            .unwrap_or(&self.header.namespace);
        let result = match (namespace, self.header.message_type.as_deref()) {
            ("playbackSession", Some("sessionStatus")) => {
                SessionStatus::deserialize(&self.body).map(Event::SessionStatus)
            }
            ("playbackSession", Some("sessionError")) => {
                let session = self.header.session_id.as_deref().ok_or(Error::Malformed)?;
                if self
                    .body
                    .get("sessionId")
                    .is_some_and(|id| id.as_str() != Some(session))
                {
                    return Err(Error::Malformed);
                }
                SessionError::deserialize(&self.body).map(Event::SessionError)
            }
            ("playbackSession", Some("sessionInfo")) => {
                SessionInfo::deserialize(&self.body).map(Event::SessionInfo)
            }
            ("playback", Some("playbackStatus")) => {
                PlaybackStatus::deserialize(&self.body).map(Event::PlaybackStatus)
            }
            ("playback", Some("playbackError")) => {
                PlaybackError::deserialize(&self.body).map(Event::PlaybackError)
            }
            _ => return Ok(None),
        };
        result.map(Some).map_err(|_| Error::Malformed)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FragmentKind {
    Text,
    Continuation,
}

#[derive(Default)]
pub struct MessageAssembler {
    bytes: Vec<u8>,
    active: bool,
}

impl MessageAssembler {
    /// Feed logical text/continuation fragments, not raw WebSocket frames.
    /// The adapter handles control frames and resets on disconnect or receive timeout.
    pub fn push(
        &mut self,
        kind: FragmentKind,
        bytes: &[u8],
        fin: bool,
    ) -> Result<Option<Vec<u8>>, Error> {
        let error = match kind {
            FragmentKind::Text if self.active => Some(Error::InvalidFragment),
            FragmentKind::Continuation if !self.active => Some(Error::InvalidFragment),
            _ if bytes.len() > MAX_MESSAGE_BYTES - self.bytes.len() => Some(Error::TooLarge),
            _ => None,
        };
        if let Some(error) = error {
            self.reset();
            return Err(error);
        }
        if self.bytes.capacity() == 0 {
            self.bytes.reserve_exact(MAX_MESSAGE_BYTES);
        }
        self.bytes.extend_from_slice(bytes);
        self.active = !fin;
        if !fin {
            return Ok(None);
        }
        if std::str::from_utf8(&self.bytes).is_err() {
            self.reset();
            return Err(Error::Malformed);
        }
        Ok(Some(std::mem::take(&mut self.bytes)))
    }
    pub fn reset(&mut self) {
        self.bytes.clear();
        self.active = false;
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
        assert_eq!(
            SonosConfig::default(),
            SonosConfig {
                player_id: None,
                autoplay: false
            }
        );
    }

    fn command() -> Command<'static> {
        Command {
            namespace: "playbackSession:1",
            command: "loadStreamUrl",
            household_id: "Sonos_EXAMPLE",
            cmd_id: "7",
            scope: Scope::Session("session-example"),
        }
    }

    fn pending() -> PendingCommand<'static> {
        PendingCommand {
            namespace: "playbackSession:1",
            command: "loadStreamUrl",
            cmd_id: "7",
            sent_at_ms: 100,
            deadline_ms: 1000,
        }
    }

    fn reply() -> &'static [u8] {
        br#"[{"namespace":"playbackSession:1","response":"loadStreamUrl","cmdId":"7","success":true},{}]"#
    }

    #[test]
    fn command_serializes_scoped_array_with_escaped_body() {
        let body = serde_json::json!({"streamUrl":"http://192.0.2.10:8000/stream.flac", "stationMetadata":{"name":"A \"record\""}});
        let bytes = encode_command(&command(), &body).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            value,
            serde_json::json!([
                {"namespace":"playbackSession:1", "command":"loadStreamUrl", "householdId":"Sonos_EXAMPLE", "cmdId":"7", "sessionId":"session-example"},
                {"streamUrl":"http://192.0.2.10:8000/stream.flac", "stationMetadata":{"name":"A \"record\""}}
            ])
        );
        for (scope, key) in [
            (Scope::Group("group:123"), "groupId"),
            (Scope::Player("player"), "playerId"),
        ] {
            let c = Command { scope, ..command() };
            let v: serde_json::Value =
                serde_json::from_slice(&encode_command(&c, &serde_json::json!({})).unwrap())
                    .unwrap();
            assert!(v[0][key].is_string());
        }
    }

    #[test]
    fn command_and_message_limits_apply_before_parsing_or_accumulation() {
        assert_eq!(
            encode_command(&command(), &serde_json::json!({"x":"x".repeat(8192)})),
            Err(Error::TooLarge)
        );
        assert_eq!(
            encode_command(&command(), &serde_json::json!(null)),
            Err(Error::Malformed)
        );
        assert_eq!(
            encode_command(
                &Command {
                    cmd_id: "",
                    ..command()
                },
                &serde_json::json!({})
            ),
            Err(Error::Malformed)
        );
        assert!(matches!(
            parse_message(&vec![b' '; 8193]),
            Err(Error::TooLarge)
        ));
        for input in [
            b"{}".as_slice(),
            b"[{},{}]",
            b"[{}, {}, {}]",
            b"not-json",
            b"[{\"namespace\":\"x\"},null]",
            b"[{\"namespace\":\"x\",\"success\":true,\"cmdId\":\"7\"},{}]",
        ] {
            assert!(parse_message(input).is_err());
        }
        let nested = format!(
            "[{{\"namespace\":\"x\"}},{{\"x\":{}0{}}}]",
            "[".repeat(40),
            "]".repeat(40)
        );
        assert!(parse_message(nested.as_bytes()).is_err());
        let mut exact = reply().to_vec();
        exact.resize(8192, b' ');
        assert!(parse_message(&exact).is_ok());
    }

    #[test]
    fn message_token_budget_counts_values_keys_and_empty_containers() {
        // Envelope array, header object, namespace key/value and body object: five tokens.
        // The x key and its array add two. Each array entry below is one token.
        for entry in ["0", "true", "null", "\"\"", "[]", "{}"] {
            let message = |count| {
                format!(
                    r#"[{{"namespace":"x"}},{{"x":[{}]}}]"#,
                    vec![entry; count].join(",")
                )
            };
            assert!(parse_message(message(249).as_bytes()).is_ok(), "{entry}");
            assert!(
                matches!(parse_message(message(250).as_bytes()), Err(Error::TooLarge)),
                "{entry}"
            );
        }
        // Five envelope tokens plus one key and one value per member: 126 is too many.
        let object = |count| {
            format!(
                r#"[{{"namespace":"x"}},{{{}}}]"#,
                (0..count)
                    .map(|i| format!(r#""k{i}":0"#))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        assert!(parse_message(object(125).as_bytes()).is_ok());
        assert!(matches!(
            parse_message(object(126).as_bytes()),
            Err(Error::TooLarge)
        ));
    }

    #[test]
    fn message_token_budget_ignores_punctuation_and_escapes_inside_strings() {
        let quoted = serde_json::to_string("\\\"[{},:] true null \\").unwrap();
        let message = |count| {
            format!(
                r#"[{{"namespace":"x"}},{{"x":[{}]}}]"#,
                vec![quoted.as_str(); count].join(",")
            )
        };
        assert!(parse_message(message(249).as_bytes()).is_ok());
        assert!(matches!(
            parse_message(message(250).as_bytes()),
            Err(Error::TooLarge)
        ));
        let numbers = format!(
            r#"[{{"namespace":"x"}},{{"x":[{}]}}]"#,
            vec!["-1.25e+3"; 249].join(",")
        );
        assert!(parse_message(numbers.as_bytes()).is_ok());
    }

    #[test]
    fn message_token_budget_rejects_dense_input_before_json_validation() {
        let message = format!(
            r#"[{{"namespace":"playback:1","type":"playbackStatus"}},{{"playbackState":"PLAYBACK_STATE_PLAYING","x":[{}]}}]"#,
            vec!["[0]"; 1900].join(",")
        );
        assert!(message.len() < MAX_MESSAGE_BYTES);
        assert!(matches!(
            parse_message(message.as_bytes()),
            Err(Error::TooLarge)
        ));
        // Bad trailing syntax does not reach serde after the budget is exhausted.
        assert!(matches!(
            parse_message(format!("{message}?").as_bytes()),
            Err(Error::TooLarge)
        ));
    }

    #[test]
    fn message_rejects_invalid_header_scopes_and_event_names() {
        for field in [
            "householdId",
            "groupId",
            "sessionId",
            "playerId",
            "locationId",
            "name",
        ] {
            for value in [
                "".to_owned(),
                "x".repeat(257),
                "with space".to_owned(),
                "line\nbreak".to_owned(),
            ] {
                let bytes = serde_json::to_vec(&serde_json::json!([
                    {"namespace":"playbackSession:1", (field):value}, {}
                ]))
                .unwrap();
                assert!(
                    matches!(parse_message(&bytes), Err(Error::Malformed)),
                    "{field}"
                );
            }
        }
    }

    #[test]
    fn observed_session_info_and_playback_error_are_not_ignored() {
        // Shapes from the local captures; identifiers are deliberately fictional.
        let info = parse_message(br#"[{"namespace":"playbackSession:1","sessionId":"ours","name":"sessionInfo","type":"sessionInfo"},{"_objectType":"sessionInfo","suspended":false}]"#).unwrap();
        assert_eq!(info.header.session_id.as_deref(), Some("ours"));
        assert_eq!(info.header.name.as_deref(), Some("sessionInfo"));
        assert_eq!(
            info.event().unwrap(),
            Some(Event::SessionInfo(SessionInfo { suspended: false }))
        );
        let error = parse_message(br#"[{"namespace":"playback:1","groupId":"group:123","name":"playbackError","type":"playbackError"},{"_objectType":"playbackError","errorCode":"ERROR_PLAYBACK_NO_PLAYABLE_CONTENT","reason":"ERROR_NO_PLAYABLE_CONTENT","itemId":"our-item","serviceId":-1}]"#).unwrap();
        assert_eq!(error.header.group_id.as_deref(), Some("group:123"));
        assert_eq!(
            error.event().unwrap(),
            Some(Event::PlaybackError(PlaybackError {
                error_code: "ERROR_PLAYBACK_NO_PLAYABLE_CONTENT".into(),
                reason: Some("ERROR_NO_PLAYABLE_CONTENT".into()),
                item_id: Some("our-item".into()),
                service_id: Some(-1),
            }))
        );
    }

    #[test]
    fn message_preserves_scopes_and_accepts_the_shared_id_boundary() {
        let id = "x".repeat(256);
        let bytes = serde_json::to_vec(&serde_json::json!([
            {"namespace":"info:1", "householdId":id, "groupId":"group:123", "sessionId":"ours", "playerId":"player", "locationId":"location", "response":"getInfo", "cmdId":"7", "success":true}, {}
        ])).unwrap();
        let header = parse_message(&bytes).unwrap().header;
        assert_eq!(header.household_id.as_deref(), Some(id.as_str()));
        assert_eq!(header.group_id.as_deref(), Some("group:123"));
        assert_eq!(header.session_id.as_deref(), Some("ours"));
        assert_eq!(header.player_id.as_deref(), Some("player"));
        assert_eq!(header.location_id.as_deref(), Some("location"));
    }

    #[test]
    fn replies_require_id_namespace_command_success_and_live_deadline() {
        let m = parse_message(reply()).unwrap();
        assert_eq!(m.match_reply(&pending(), 999), ReplyMatch::Success);
        assert_eq!(m.match_reply(&pending(), 1000), ReplyMatch::Expired);
        assert_eq!(m.match_reply(&pending(), 99), ReplyMatch::Expired);
        for p in [
            PendingCommand {
                cmd_id: "8",
                ..pending()
            },
            PendingCommand {
                namespace: "other",
                ..pending()
            },
            PendingCommand {
                command: "other",
                ..pending()
            },
        ] {
            assert_eq!(m.match_reply(&p, 200), ReplyMatch::Unrelated);
        }
        let failed = String::from_utf8(reply().to_vec())
            .unwrap()
            .replace("true", "false");
        assert_eq!(
            parse_message(failed.as_bytes())
                .unwrap()
                .match_reply(&pending(), 200),
            ReplyMatch::Failure
        );
        assert_eq!(m.event().unwrap(), None);
    }

    #[test]
    fn captured_eviction_uses_authoritative_header_session_scope() {
        // Reconstructed from the 2026-10-07 parsed header/body diagnostics, not raw wire bytes.
        // IDs are sanitized; both supported namespace spellings are checked below.
        let mut message = parse_message(br#"[{"namespace":"playbackSession:1","name":"sessionError","householdId":"house","sessionId":"owned","locationId":"location","type":"sessionError"},{"_objectType":"sessionError","errorCode":"ERROR_SESSION_EVICTED","reason":"Replaced"}]"#).unwrap();
        assert_eq!(message.match_reply(&pending(), 200), ReplyMatch::Unrelated);
        assert_eq!(
            message.event(),
            Ok(Some(Event::SessionError(SessionError {
                error_code: "ERROR_SESSION_EVICTED".into(),
            })))
        );
        let expected = message.event();
        message.header.namespace = "playbackSession".into();
        assert_eq!(message.event(), expected);
        message.body["sessionId"] = serde_json::json!("owned");
        assert_eq!(message.event(), expected);
        for value in [
            serde_json::json!("other"),
            serde_json::Value::Null,
            serde_json::json!(3),
        ] {
            message.body["sessionId"] = value;
            assert_eq!(message.event(), Err(Error::Malformed));
        }
        message.header.session_id = None;
        message.body["sessionId"] = serde_json::json!("owned");
        assert_eq!(message.event(), Err(Error::Malformed));
    }

    #[test]
    fn unsolicited_events_are_typed_and_never_command_success() {
        let m = parse_message(br#"[{"namespace":"playbackSession:1","sessionId":"ours","type":"sessionError"},{"errorCode":"ERROR_SESSION_EVICTED"}]"#).unwrap();
        assert_eq!(m.match_reply(&pending(), 200), ReplyMatch::Unrelated);
        assert_eq!(
            m.event().unwrap(),
            Some(Event::SessionError(SessionError {
                error_code: "ERROR_SESSION_EVICTED".into()
            }))
        );
        let status = parse_message(br#"[{"namespace":"playbackSession","type":"sessionStatus"},{"sessionId":"ours","sessionState":"SESSION_STATE_ACTIVE"}]"#).unwrap();
        assert!(matches!(
            status.event().unwrap(),
            Some(Event::SessionStatus(_))
        ));
        let playback = parse_message(br#"[{"namespace":"playback:1","type":"playbackStatus"},{"playbackState":"PLAYBACK_STATE_PLAYING","itemId":"our-item"}]"#).unwrap();
        assert!(matches!(
            playback.event().unwrap(),
            Some(Event::PlaybackStatus(PlaybackStatus {
                item_id: Some(_),
                ..
            }))
        ));
        let wrong = parse_message(br#"[{"namespace":"other","type":"sessionError"},{}]"#).unwrap();
        assert_eq!(wrong.event().unwrap(), None);
        let malformed =
            parse_message(br#"[{"namespace":"playbackSession","type":"sessionError"},{}]"#)
                .unwrap();
        assert_eq!(malformed.event(), Err(Error::Malformed));
    }

    #[test]
    fn assembler_accepts_split_utf8_and_rejects_bad_order_size_and_encoding() {
        let mut a = MessageAssembler::default();
        assert_eq!(a.push(FragmentKind::Text, &[0xc3], false), Ok(None));
        assert_eq!(
            a.push(FragmentKind::Continuation, &[0xa9], true),
            Ok(Some(vec![0xc3, 0xa9]))
        );
        assert_eq!(
            a.push(FragmentKind::Continuation, b"x", true),
            Err(Error::InvalidFragment)
        );
        assert_eq!(
            a.push(FragmentKind::Text, &[0xff], true),
            Err(Error::Malformed)
        );
        assert_eq!(a.push(FragmentKind::Text, b"a", false), Ok(None));
        assert_eq!(
            a.push(FragmentKind::Text, b"b", true),
            Err(Error::InvalidFragment)
        );
        assert_eq!(
            a.push(FragmentKind::Text, &vec![b'x'; 8192], false),
            Ok(None)
        );
        assert_eq!(
            a.push(FragmentKind::Continuation, b"x", true),
            Err(Error::TooLarge)
        );
        assert_eq!(
            a.push(FragmentKind::Text, &vec![b'x'; 8193], true),
            Err(Error::TooLarge)
        );
        assert_eq!(
            a.push(FragmentKind::Text, b"ok", true),
            Ok(Some(b"ok".to_vec()))
        );
        assert_eq!(a.push(FragmentKind::Text, b"a", false), Ok(None));
        a.reset();
        assert_eq!(
            a.push(FragmentKind::Text, b"b", true),
            Ok(Some(b"b".to_vec()))
        );
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
