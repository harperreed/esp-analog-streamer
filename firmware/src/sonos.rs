// ABOUTME: One bounded Sonos controller worker owns discovery, HTTP control and our playback intent.
// ABOUTME: The UI queues commands and reads snapshots without waiting for speaker I/O.

use esp_shazam_core::network_lifecycle::{Lease, Network};
use esp_shazam_core::sonos::{self as protocol, DiscoveryGroup, SonosConfig};
use esp_shazam_core::sonos_queue::{self, QueueSnapshot};
use serde::Serialize;

#[derive(Debug)]
pub enum Command {
    Discover,
    Select(String),
    Play,
    Stop,
    SetAutoplay(bool),
    SetEnabled(bool),
    SetFormat(esp_shazam_core::stream_buffer::OutputFormat),
    SetBufferPreset(esp_shazam_core::stream_buffer::BufferPreset),
}

/// Why `Handle::submit` refused a command. The HTTP handler maps each to a status and body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Sonos control is off. Stop and Enable pass so an off controller can still be cancelled or turned on.
    Disabled,
    /// Startup history needs the memory recognition holds.
    BufferUnavailable,
    /// MP3 needs the memory recognition holds.
    Mp3Unavailable,
    /// The destination identity fails the discovery contract.
    InvalidPlayer,
    /// Four commands already wait for the worker.
    QueueFull,
    /// The worker could not be launched or has exited.
    WorkerUnavailable,
    /// The cancellation generation counter ran out.
    GenerationExhausted,
}

/// The one admission rule for every command, read from one status snapshot. `buffer_available`
/// is the boot-time inverse of recognition, so it also gates MP3.
fn admit(command: &Command, status: &Snapshot) -> Result<(), Refusal> {
    use esp_shazam_core::stream_buffer::OutputFormat;
    if !status.config.enabled && !matches!(command, Command::Stop | Command::SetEnabled(_)) {
        return Err(Refusal::Disabled);
    }
    match command {
        Command::SetBufferPreset(_) if !status.buffer_available => Err(Refusal::BufferUnavailable),
        Command::SetFormat(OutputFormat::Mp3) if !status.buffer_available => {
            Err(Refusal::Mp3Unavailable)
        }
        Command::Select(player) => {
            protocol::validate_id(player).map_err(|_| Refusal::InvalidPlayer)
        }
        _ => Ok(()),
    }
}

struct QueuedCommand {
    command: Command,
    generation: u64,
    network: Option<Lease>,
}

impl QueuedCommand {
    fn network_current(&self, network: &Network) -> bool {
        !matches!(self.command, Command::Play | Command::Discover)
            || self.network.is_some_and(|lease| network.is_current(lease))
    }
}

struct Commands {
    network: Network,
    sender: std::sync::Mutex<(
        std::sync::mpsc::SyncSender<QueuedCommand>,
        u64,
        std::sync::Arc<std::sync::Mutex<bool>>,
    )>,
}

impl Commands {
    fn new(network: Network) -> (Self, std::sync::mpsc::Receiver<QueuedCommand>) {
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        (
            Self {
                network,
                sender: std::sync::Mutex::new((
                    sender,
                    0,
                    std::sync::Arc::new(std::sync::Mutex::new(true)),
                )),
            },
            receiver,
        )
    }

    fn submit(&self, command: Command) -> Result<(), Refusal> {
        let mut sender = self.sender.lock().unwrap();
        let cancels = matches!(
            command,
            Command::Stop
                | Command::Select(_)
                | Command::SetAutoplay(false)
                | Command::SetEnabled(false)
        );
        let generation = if cancels {
            sender
                .1
                .checked_add(1)
                .ok_or(Refusal::GenerationExhausted)?
        } else {
            sender.1
        };
        sender
            .0
            .try_send(QueuedCommand {
                command,
                generation,
                network: self.network.lease(),
            })
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => Refusal::QueueFull,
                std::sync::mpsc::TrySendError::Disconnected(_) => Refusal::WorkerUnavailable,
            })?;
        if cancels {
            *sender.2.lock().unwrap() = false;
            sender.2 = std::sync::Arc::new(std::sync::Mutex::new(true));
        }
        sender.1 = generation;
        Ok(())
    }

    fn token(
        &self,
        generation: u64,
    ) -> Result<std::sync::Arc<std::sync::Mutex<bool>>, &'static str> {
        let sender = self.sender.lock().unwrap();
        if sender.1 != generation {
            return Err("Sonos Play cancelled");
        }
        Ok(sender.2.clone())
    }

    /// Serializes one bounded final send with acceptance of commands that cancel Play.
    fn run_if_current<T>(
        &self,
        generation: u64,
        operation: impl FnOnce() -> T,
    ) -> Result<T, &'static str> {
        let token = self.token(generation)?;
        let permit = token.lock().unwrap();
        if !*permit {
            return Err("Sonos Play cancelled");
        }
        Ok(operation())
    }

    fn is_current(&self, generation: u64) -> bool {
        self.sender.lock().unwrap().1 == generation
    }
}

/// The network lock covers checks only; platform I/O retains its existing deadlines.
fn with_network<T>(
    network: &Network,
    lease: Lease,
    operation: impl FnOnce() -> T,
) -> Result<T, &'static str> {
    if !network.is_current(lease) {
        return Err("Sonos network changed");
    }
    let result = operation();
    if !network.is_current(lease) {
        return Err("Sonos network changed");
    }
    Ok(result)
}

/// Stop relinquishes local ownership even when the old network cannot confirm suspension.
fn stop_completion<E>(
    network: &Network,
    lease: Option<Lease>,
    result: Result<(), E>,
) -> Result<(), E> {
    if lease.is_some_and(|lease| !network.is_current(lease)) {
        Ok(())
    } else {
        result
    }
}

/// Keeps launch resources outside a fallible thread closure until the worker owns them.
type WorkerResources<T> = std::sync::Arc<std::sync::Mutex<Option<T>>>;

struct WorkerStart<T> {
    pending: std::sync::Mutex<Option<WorkerResources<T>>>,
}

impl<T> WorkerStart<T> {
    fn new(resources: T) -> Self {
        Self {
            pending: std::sync::Mutex::new(Some(std::sync::Arc::new(std::sync::Mutex::new(Some(
                resources,
            ))))),
        }
    }

    fn ensure_started<E>(
        &self,
        spawn: impl FnOnce(WorkerResources<T>) -> Result<(), E>,
    ) -> Result<(), E> {
        // Serialize creation; only an actual successful launch consumes the pending owner.
        let mut pending = self.pending.lock().unwrap();
        if let Some(resources) = pending.as_ref() {
            spawn(resources.clone())?;
            *pending = None;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Stopped,
    Discovering,
    Connecting,
    Buffering,
    Playing,
    Degraded,
    Recovering,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub config: SonosConfig,
    pub buffer_available: bool,
    pub active_format: esp_shazam_core::stream_buffer::OutputFormat,
    pub groups: Vec<DiscoveryGroup>,
    pub active_player_id: Option<String>,
    pub state: State,
    pub last_error: Option<String>,
}

fn save_config<E>(
    status: &std::sync::Mutex<Snapshot>,
    config: SonosConfig,
    persist: impl FnOnce(&SonosConfig) -> Result<(), E>,
) -> Result<(), E> {
    persist(&config)?;
    let mut status = status.lock().unwrap();
    if !matches!(status.state, State::Degraded | State::Recovering) {
        status.last_error = None;
    }
    status.config = config;
    Ok(())
}

fn queue_for_play(
    item: String,
    host: std::net::Ipv4Addr,
    status: &Snapshot,
    network_generation: u32,
    sonos_intent: u64,
) -> Result<QueueSnapshot, sonos_queue::Error> {
    QueueSnapshot::new(
        item,
        host,
        status
            .buffer_available
            .then_some(status.config.buffer_preset),
        status.active_format,
        network_generation,
        sonos_intent,
    )
}

fn start_autoplay<E: std::fmt::Display>(
    status: &std::sync::Mutex<Snapshot>,
    start: impl FnOnce() -> Result<(), E>,
) {
    let enabled = status.lock().unwrap().config.autoplay();
    if enabled {
        if let Err(error) = start() {
            let mut snapshot = status.lock().unwrap();
            snapshot.state = State::Error;
            snapshot.last_error = Some(format!("Cannot start Sonos worker: {error}"));
        }
    }
}

pub fn http_transport_state(state: esp_shazam_core::sonos_upnp::TransportState) -> State {
    use esp_shazam_core::sonos_upnp::TransportState;
    match state {
        TransportState::Playing => State::Playing,
        TransportState::Transitioning => State::Buffering,
        TransportState::Stopped | TransportState::Paused => State::Stopped,
    }
}

/// Exact URI evidence gates the existing explicit Stop path.
pub struct HttpOwnership {
    uri: String,
}
impl HttpOwnership {
    pub fn new(uri: String) -> Self {
        Self { uri }
    }
    pub fn matches(&self, uri: &str) -> bool {
        self.uri == uri
    }
}

fn sample_level(sample: Option<(f32, std::time::Instant)>, now: std::time::Instant) -> f32 {
    sample
        .filter(|(level, measured)| {
            level.is_finite()
                && now
                    .checked_duration_since(*measured)
                    .is_some_and(|age| age <= std::time::Duration::from_secs(2))
        })
        .map_or(f32::NAN, |(level, _)| level)
}

#[derive(Default)]
struct LevelSamples {
    last: Option<std::time::Instant>,
}

impl LevelSamples {
    fn update(
        &mut self,
        policy: &mut protocol::Autoplay,
        epoch: std::time::Instant,
        sample: Option<(f32, std::time::Instant)>,
        now: std::time::Instant,
    ) -> protocol::Action {
        let level = sample_level(sample, now);
        let measured = sample.map(|(_, measured)| measured);
        if !level.is_finite() || measured.is_some_and(|at| at < epoch) {
            return policy.update(
                now.duration_since(epoch).as_millis() as u64,
                protocol::AutoplayEvent::Level(f32::NAN),
            );
        }
        let measured = measured.expect("Finite sample has a timestamp");
        if self.last.is_some_and(|last| measured <= last) {
            return protocol::Action::None;
        }
        let ms = measured.duration_since(epoch).as_millis() as u64;
        if self
            .last
            .is_some_and(|last| measured.duration_since(last) > std::time::Duration::from_secs(2))
        {
            policy.update(ms, protocol::AutoplayEvent::Level(f32::NAN));
        }
        self.last = Some(measured);
        policy.update(ms, protocol::AutoplayEvent::Level(level))
    }
}

fn resolve_group(advertisements: &[DiscoveryGroup], player: &str) -> Option<DiscoveryGroup> {
    let selected = advertisements.iter().find(|g| g.player_id == player)?;
    advertisements
        .iter()
        .find(|g| {
            g.is_leader
                && g.household_id == selected.household_id
                && g.group_id == selected.group_id
        })
        .cloned()
}

fn decode_config(saved: Option<&str>) -> (SonosConfig, Option<String>) {
    let Some(saved) = saved else {
        return (SonosConfig::default(), None);
    };
    match serde_json::from_str::<SonosConfig>(saved) {
        Ok(config)
            if config
                .player_id
                .as_deref()
                .is_none_or(|id| protocol::validate_id(id).is_ok()) =>
        {
            (config, None)
        }
        _ => (
            SonosConfig::default(),
            Some("Invalid saved Sonos configuration; select a destination to repair it".into()),
        ),
    }
}

#[derive(Clone)]
struct Queue {
    snapshot: std::sync::Arc<std::sync::Mutex<QueueState>>,
    capacity: std::sync::Arc<esp_shazam_core::stream_wait::Capacity>,
    network: Network,
}

#[derive(Default)]
struct QueueState {
    snapshot: Option<QueueSnapshot>,
    intent: Option<u64>,
}

// Clears only the Play that installed this reservation on any failed startup path.
struct IntentGuard {
    queue: Queue,
    id: u64,
    armed: bool,
}
impl Drop for IntentGuard {
    fn drop(&mut self) {
        if self.armed {
            self.queue.clear_intent(self.id);
        }
    }
}

impl Queue {
    fn new(network: Network) -> Self {
        Self {
            snapshot: Default::default(),
            #[cfg(target_os = "espidf")]
            capacity: crate::stream::capacity(),
            #[cfg(not(target_os = "espidf"))]
            capacity: Default::default(),
            network,
        }
    }
    fn snapshot(&self) -> Option<QueueSnapshot> {
        let snapshot = self.snapshot.lock().unwrap().snapshot.clone()?;
        if !self.capacity.valid(snapshot.sonos_intent) {
            return None;
        }
        self.network
            .is_current(Lease {
                generation: snapshot.network_generation,
                address: snapshot.host,
            })
            .then_some(snapshot)
    }
    fn publish(&self, snapshot: QueueSnapshot) {
        self.snapshot.lock().unwrap().snapshot = Some(snapshot);
    }
    fn clear(&self) {
        let id = {
            let mut state = self.snapshot.lock().unwrap();
            state.snapshot = None;
            state.intent.take()
        };
        if let Some(id) = id {
            self.capacity.clear(id);
        }
    }
    fn clear_intent(&self, id: u64) {
        let mut state = self.snapshot.lock().unwrap();
        if state.intent == Some(id) {
            state.intent = None;
            state.snapshot = None;
        }
        drop(state);
        self.capacity.clear(id);
    }
    fn intent_valid(&self) -> bool {
        let id = self.snapshot.lock().unwrap().intent;
        id.is_some_and(|id| self.capacity.valid(id))
    }
    fn revoke_cancelled(&self, commands: &Commands, generation: u64) -> bool {
        if commands.is_current(generation) {
            return false;
        }
        self.clear();
        true
    }
    /// Caller holds the command token gate; an admitted Sonos listener needs no free slot.
    fn play_current(&self, intent: u64) -> bool {
        self.capacity.current(intent)
    }
    /// Caller holds the command token gate; do not lock that token again.
    #[cfg(test)]
    fn intent_current(&self) -> bool {
        let id = self.snapshot.lock().unwrap().intent;
        id.is_some_and(|id| self.capacity.current(id))
    }
}

/// Three read-only retries retain the original delivery, never its own copy of an intent.
#[derive(Debug, PartialEq, Eq)]
struct ControlRecovery {
    deadline: std::time::Instant,
    next: std::time::Instant,
    attempts: u8,
    random: u32,
}

impl ControlRecovery {
    fn new(now: std::time::Instant, seed: u32) -> Self {
        let mut recovery = Self {
            deadline: now + std::time::Duration::from_secs(30),
            next: now,
            attempts: 0,
            random: seed,
        };
        recovery.backoff(now).expect("Initial recovery budget");
        recovery
    }

    fn backoff(&mut self, now: std::time::Instant) -> std::io::Result<()> {
        if self.attempts >= 3 || now >= self.deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Sonos recovery exhausted",
            ));
        }
        self.random = self.random.wrapping_mul(1664525).wrapping_add(1013904223);
        let base = 1000u64 << self.attempts;
        let delay = base * 800 / 1000 + u64::from(self.random) % (base * 400 / 1000 + 1);
        self.next = now + std::time::Duration::from_millis(delay);
        Ok(())
    }

    fn begin(&mut self, now: std::time::Instant) -> std::io::Result<std::time::Instant> {
        if now < self.next || now >= self.deadline || self.attempts >= 3 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Sonos recovery exhausted",
            ));
        }
        self.attempts += 1;
        Ok(self.deadline.min(now + std::time::Duration::from_secs(5)))
    }
}

/// One reconnect attempt after a lost session, due at `at`.
#[derive(Debug, PartialEq, Eq)]
struct Retry {
    player: String,
    at: std::time::Instant,
    attempt: usize,
}

/// The worker's connection phase. Every teardown consumes the old phase, so a client without
/// its lease, or a retry beside a live client, cannot exist. `C` is the control client; tests
/// use `()`.
#[derive(Debug, PartialEq, Eq)]
enum Session<C = esp_shazam_core::sonos_upnp::Client> {
    Idle,
    /// The last session failed; observe the selected player again at `Retry::at`, never take over.
    Reconnecting(Retry),
    Connected {
        client: C,
        lease: Lease,
    },
    /// Our queue is on the player at `uri`; `recovery` is the bounded read-only retry after a
    /// control-read failure, kept with the URI it observes.
    Owned {
        client: C,
        lease: Lease,
        uri: String,
        recovery: Option<ControlRecovery>,
    },
}

impl<C> Session<C> {
    /// A lost session retries the selected player in a second, or rests when none is selected.
    /// A failed transport is not permission to take over again: the client goes with it.
    fn lost(self, selected: Option<String>, now: std::time::Instant) -> Self {
        match self {
            Session::Connected { .. } | Session::Owned { .. } => match selected {
                Some(player) => Session::Reconnecting(Retry {
                    player,
                    at: now + std::time::Duration::from_secs(1),
                    attempt: 0,
                }),
                None => Session::Idle,
            },
            Session::Idle | Session::Reconnecting(_) => Session::Idle,
        }
    }

    /// A failed reconnect waits twice as long each time and gives up after the third.
    fn retry_later(mut retry: Retry, now: std::time::Instant) -> Self {
        retry.attempt += 1;
        if retry.attempt < 3 {
            retry.at = now + std::time::Duration::from_secs(1 << retry.attempt);
            Session::Reconnecting(retry)
        } else {
            Session::Idle
        }
    }

    /// Cancellation drops a stale retry and the recovery budget, but keeps a live client and the
    /// URI evidence for the queued checked Stop.
    fn cancelled(self) -> Self {
        match self {
            Session::Reconnecting(_) => Session::Idle,
            Session::Owned {
                client,
                lease,
                uri,
                recovery: _,
            } => Session::Owned {
                client,
                lease,
                uri,
                recovery: None,
            },
            other => other,
        }
    }

    /// The published queue at `uri` is ours; without a live client nothing changes.
    fn own(self, uri: String) -> Self {
        match self {
            Session::Connected { client, lease } | Session::Owned { client, lease, .. } => {
                Session::Owned {
                    client,
                    lease,
                    uri,
                    recovery: None,
                }
            }
            other => other,
        }
    }

    /// The player stopped our queue: keep the client, forget the URI and any recovery.
    fn released(self) -> Self {
        match self {
            Session::Owned { client, lease, .. } => Session::Connected { client, lease },
            other => other,
        }
    }

    /// Read-only recovery is in flight; autoplay timers and error clearing wait for it.
    fn recovering(&self) -> bool {
        matches!(
            self,
            Session::Owned {
                recovery: Some(_),
                ..
            }
        )
    }

    /// Takes the retry once it is due; every other phase stays as it is.
    fn due_retry(&mut self, now: std::time::Instant) -> Option<Retry> {
        match std::mem::replace(self, Session::Idle) {
            Session::Reconnecting(retry) if now >= retry.at => Some(retry),
            other => {
                *self = other;
                None
            }
        }
    }

    /// The lease the live client's I/O runs under.
    fn lease(&self) -> Option<Lease> {
        match self {
            Session::Connected { lease, .. } | Session::Owned { lease, .. } => Some(*lease),
            Session::Idle | Session::Reconnecting(_) => None,
        }
    }
}

fn recoverable_observation_error(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    matches!(
        error.kind(),
        ErrorKind::TimedOut
            | ErrorKind::WouldBlock
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionRefused
            | ErrorKind::NotConnected
            | ErrorKind::BrokenPipe
    )
}

fn observation_checkpoint(
    queue: &Queue,
    commands: &Commands,
    generation: u64,
    network: &Network,
    lease: Lease,
) -> std::io::Result<()> {
    if !commands.is_current(generation) || !network.is_current(lease) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Sonos observation cancelled or network changed",
        ));
    }
    if !queue.intent_valid() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Sonos reservation expired or cancelled",
        ));
    }
    Ok(())
}

/// Final status publication shares the command cancellation gate, with no I/O under it.
fn commit_observation<T>(
    queue: &Queue,
    commands: &Commands,
    generation: u64,
    network: &Network,
    lease: Lease,
    commit: impl FnOnce() -> T,
) -> std::io::Result<T> {
    observation_checkpoint(queue, commands, generation, network, lease)?;
    commands
        .run_if_current(generation, || {
            let intent = queue.snapshot.lock().unwrap().intent;
            if !network.is_current(lease) || !intent.is_some_and(|id| queue.play_current(id)) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "Sonos observation no longer current",
                ));
            }
            Ok(commit())
        })
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Interrupted, error))?
}

/// Applies read-only control results to the sole live delivery and public state.
struct ControlObservation<'a> {
    queue: &'a Queue,
    commands: &'a Commands,
    generation: u64,
    lease: Lease,
    status: &'a std::sync::Mutex<Snapshot>,
}

impl ControlObservation<'_> {
    fn finish(
        &self,
        recovery: &mut Option<ControlRecovery>,
        observed: std::io::Result<Option<esp_shazam_core::sonos_upnp::TransportState>>,
        now: std::time::Instant,
        seed: u32,
    ) -> std::io::Result<State> {
        observation_checkpoint(
            self.queue,
            self.commands,
            self.generation,
            &self.queue.network,
            self.lease,
        )?;
        let error_message = observed.as_ref().err().map(ToString::to_string);
        let result = (|| {
            if recovery.as_ref().is_some_and(|retry| now >= retry.deadline) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Sonos recovery exhausted",
                ));
            }
            match observed {
                Ok(None) => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Playback source changed",
                )),
                Ok(Some(transport)) => {
                    let state = http_transport_state(transport);
                    if state == State::Buffering {
                        if let Some(retry) = recovery.as_mut() {
                            retry.backoff(now)?;
                            return Ok(State::Degraded);
                        }
                    }
                    Ok(state)
                }
                Err(error) => {
                    if !recoverable_observation_error(&error) {
                        return Err(error);
                    }
                    if let Some(retry) = recovery.as_mut() {
                        retry.backoff(now)?;
                    } else {
                        if !matches!(
                            self.status.lock().unwrap().state,
                            State::Playing | State::Buffering
                        ) {
                            return Err(error);
                        }
                        *recovery = Some(ControlRecovery::new(now, seed));
                    }
                    Ok(State::Degraded)
                }
            }
        })();
        let state = match result {
            Ok(state) => state,
            Err(error) => {
                self.queue.clear();
                *recovery = None;
                return Err(error);
            }
        };
        commit_observation(
            self.queue,
            self.commands,
            self.generation,
            &self.queue.network,
            self.lease,
            || {
                let mut status = self.status.lock().unwrap();
                status.state = state;
                if state == State::Playing {
                    status.last_error = None;
                }
                if state == State::Degraded {
                    if let Some(error) = error_message {
                        status.last_error = Some(error);
                    }
                }
                if state == State::Stopped {
                    status.active_player_id = None;
                }
            },
        )?;
        if matches!(state, State::Playing | State::Stopped) {
            *recovery = None;
        }
        if state == State::Stopped {
            self.queue.clear();
        }
        Ok(state)
    }
}

/// Cancellation revokes local delivery while retaining the sole URI candidate for checked Stop.
fn revoke_cancelled_play(
    queue: &Queue,
    commands: &Commands,
    generation: u64,
    state: &mut State,
) -> bool {
    if !queue.revoke_cancelled(commands, generation) {
        return false;
    }
    *state = State::Stopped;
    true
}

#[cfg(target_os = "espidf")]
mod device {
    use super::*;
    use crate::recognizer;
    use anyhow::{anyhow, ensure, Context, Result};
    use esp_idf_svc::{
        nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault},
        sys,
    };
    use esp_shazam_core::sonos_upnp::Client;
    use std::{
        net::{Ipv4Addr, SocketAddr, UdpSocket},
        sync::{
            mpsc::{self, Receiver},
            Arc, Mutex,
        },
        time::{Duration, Instant},
    };
    const TIMEOUT: Duration = Duration::from_secs(5);

    #[derive(Clone)]
    pub struct Handle {
        commands: Arc<Commands>,
        status: Arc<Mutex<Snapshot>>,
        pending: Arc<WorkerStart<Launch>>,
        queue: Queue,
        network: Network,
    }

    struct Launch {
        receiver: Receiver<QueuedCommand>,
        store: recognizer::SharedStore,
        nvs: EspNvs<NvsDefault>,
    }

    impl Handle {
        pub(crate) fn metrics_network(&self) -> Option<crate::device_metrics::NetworkMetadata> {
            let snapshot = self.network.try_snapshot()?;
            Some(crate::device_metrics::NetworkMetadata {
                generation: snapshot.generation,
                connected: snapshot.address.is_some(),
                disconnects: crate::net::disconnect_count(),
            })
        }

        pub(crate) fn metrics_settings(&self) -> Option<crate::device_metrics::Settings> {
            let (next_play_format, next_play_buffer, buffer_available, autoplay) = {
                let status = self.status.try_lock().ok()?;
                (
                    status.config.output_format,
                    status.config.buffer_preset,
                    status.buffer_available,
                    status.config.autoplay(),
                )
            };
            Some(crate::device_metrics::Settings {
                active_format: crate::stream::try_active_format()?,
                next_play_format,
                next_play_buffer,
                buffer_available,
                autoplay,
                recognition_enabled: crate::config::CONFIG.shazam_enabled,
                pga_gain: crate::input_gain::LIVE_REGISTER
                    .load(std::sync::atomic::Ordering::Relaxed),
            })
        }

        pub fn snapshot(&self) -> Snapshot {
            let mut snapshot = self.status.lock().unwrap().clone();
            snapshot.active_format = crate::stream::active_format();
            snapshot
        }

        pub fn queue_snapshot(&self) -> Option<QueueSnapshot> {
            self.queue.snapshot()
        }

        fn ensure_started(&self) -> Result<()> {
            self.pending.ensure_started(|resources| {
                let status = self.status.clone();
                let queue = self.queue.clone();
                let commands = self.commands.clone();
                let network = self.network.clone();
                crate::threads::ThreadSpec {
                    name: c"sonos",
                    core: esp_idf_svc::hal::cpu::Core::Core0,
                    priority: 2,
                    stack_bytes: 16 * 1024,
                }
                .spawn(move || {
                    let launch = resources
                        .lock()
                        .unwrap()
                        .take()
                        .expect("Sonos launch owner");
                    let mut policy =
                        protocol::Autoplay::new(-50.0, -60.0).expect("Valid Sonos thresholds");
                    policy.update(
                        0,
                        protocol::AutoplayEvent::Enabled(status.lock().unwrap().config.autoplay()),
                    );
                    Worker {
                        commands,
                        generation: 0,
                        network,
                        status,
                        nvs: launch.nvs,
                        store: launch.store,
                        session: Session::Idle,
                        queue,
                        poll_at: Instant::now(),
                        sequence: 0,
                        epoch: Instant::now(),
                        policy,
                        samples: LevelSamples::default(),
                        diagnostic_at: Instant::now(),
                    }
                    .run(launch.receiver);
                })
            })
        }

        /// The single admission authority: every gate reads one status snapshot, and a refused
        /// command never launches the worker.
        pub fn submit(&self, command: Command) -> Result<(), Refusal> {
            admit(&command, &self.status.lock().unwrap())?;
            self.ensure_started().map_err(|error| {
                log::warn!("sonos: cannot start worker: {error}");
                Refusal::WorkerUnavailable
            })?;
            self.commands.submit(command)
        }
    }

    pub fn start(
        store: recognizer::SharedStore,
        nvs: EspDefaultNvsPartition,
        network: Network,
    ) -> Result<Handle> {
        let nvs = EspNvs::new(nvs, "sonos", true)?;
        let mut bytes = [0u8; 1024];
        let (config, last_error) = match nvs.get_str("config", &mut bytes) {
            Ok(saved) => decode_config(saved),
            Err(error) => (
                SonosConfig::default(),
                Some(format!("Cannot read saved Sonos configuration: {error}")),
            ),
        };
        let config = esp_shazam_core::sonos::effective(config);
        let mut last_error = last_error;
        if let Err(error) =
            crate::stream::select_format(config.output_format, false, Arc::new(Mutex::new(true)))
        {
            last_error = Some(error.to_string());
        }
        let status = Arc::new(Mutex::new(Snapshot {
            active_format: esp_shazam_core::stream_buffer::OutputFormat::Flac,
            buffer_available: !crate::config::CONFIG.shazam_enabled,
            last_error,
            config,
            groups: Vec::new(),
            active_player_id: None,
            state: State::Stopped,
        }));
        let (commands, receiver) = Commands::new(network.clone());
        let handle = Handle {
            commands: Arc::new(commands),
            status,
            queue: Queue::new(network.clone()),
            network,
            pending: Arc::new(WorkerStart::new(Launch {
                receiver,
                nvs,
                store,
            })),
        };
        // No task stack until an explicit command, unless autoplay was deliberately persisted.
        start_autoplay(&handle.status, || handle.ensure_started());
        Ok(handle)
    }

    fn discover(
        selected: Option<&str>,
        network: &Network,
        lease: Lease,
    ) -> Result<Vec<DiscoveryGroup>> {
        let socket = with_network(network, lease, || UdpSocket::bind((lease.address, 0)))
            .map_err(|error| anyhow!(error))??;
        socket.set_read_timeout(Some(Duration::from_millis(200)))?;
        socket.set_write_timeout(Some(Duration::from_millis(200)))?;
        socket.set_multicast_ttl_v4(2)?;
        with_network(network, lease, || socket.send_to(b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: urn:smartspeaker-audio:service:SpeakerGroup:1\r\n\r\n", (Ipv4Addr::new(239,255,255,250), 1900))).map_err(|error| anyhow!(error))??;
        let deadline = Instant::now() + Duration::from_secs(3);
        // The extra byte detects truncated oversized packets instead of accepting their prefix.
        let mut packet = [0u8; protocol::MAX_DISCOVERY_BYTES + 1];
        let mut groups: Vec<DiscoveryGroup> = Vec::new();
        while Instant::now() < deadline {
            match with_network(network, lease, || socket.recv_from(&mut packet))
                .map_err(|error| anyhow!(error))?
            {
                Ok((len, SocketAddr::V4(peer))) => {
                    if let Ok(Some(group)) =
                        protocol::parse_advertisement(&packet[..len], *peer.ip())
                    {
                        if !group.is_leader && Some(group.player_id.as_str()) != selected {
                            continue;
                        }
                        if let Some(existing) =
                            groups.iter_mut().find(|g| g.player_id == group.player_id)
                        {
                            *existing = group;
                        } else if (group.is_leader
                            && groups.iter().filter(|g| g.is_leader).count() < 16)
                            || (!group.is_leader && Some(group.player_id.as_str()) == selected)
                        {
                            groups.push(group);
                        }
                    }
                }
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e.into()),
            }
        }
        groups.sort_by(|a, b| a.name.cmp(&b.name));
        with_network(network, lease, || groups).map_err(|error| anyhow!(error))
    }

    struct Worker {
        commands: Arc<Commands>,
        generation: u64,
        network: Network,
        status: Arc<Mutex<Snapshot>>,
        nvs: EspNvs<NvsDefault>,
        store: recognizer::SharedStore,
        session: Session,
        queue: Queue,
        poll_at: Instant,
        policy: protocol::Autoplay,
        samples: LevelSamples,
        diagnostic_at: Instant,
        sequence: u64,
        epoch: Instant,
    }

    impl Worker {
        fn now(&self) -> u64 {
            self.epoch.elapsed().as_millis() as u64
        }

        fn run(mut self, commands: Receiver<QueuedCommand>) {
            loop {
                match commands.recv_timeout(Duration::from_millis(50)) {
                    Ok(queued) => {
                        self.check_network();
                        if !queued.network_current(&self.network) {
                            self.status.lock().unwrap().last_error =
                                Some("Sonos network changed".into());
                            continue;
                        }
                        self.cancel_local_if_stale();
                        self.generation = queued.generation;
                        let command = queued.command;
                        let controls_playback =
                            matches!(command, Command::Play | Command::Stop | Command::Select(_));
                        if let Err(error) = self.command(command, queued.network) {
                            if controls_playback {
                                self.fail(error);
                            } else {
                                self.status.lock().unwrap().last_error = Some(error.to_string());
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = self.stop();
                        return;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                self.check_network();
                let sample = self.store.lock().unwrap().level_sample();
                let level = sample_level(sample, Instant::now());
                // Keep autoplay ownership and quiet timers intact while control is unavailable.
                let action = if !self.session.recovering() {
                    self.samples
                        .update(&mut self.policy, self.epoch, sample, Instant::now())
                } else {
                    protocol::Action::None
                };
                let result = match action {
                    protocol::Action::Start => self.play(self.network.lease()),
                    protocol::Action::Stop => self.stop(),
                    protocol::Action::None => Ok(()),
                };
                if let Err(error) = result {
                    self.fail(error);
                }
                if self.diagnostic_at.elapsed() >= Duration::from_secs(10) {
                    self.diagnostic_at = Instant::now();
                    // ESP-IDF's stack high-water API returns bytes, unlike upstream FreeRTOS.
                    let stack = unsafe { sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
                    let state = self.status.lock().unwrap().state;
                    log::info!("sonos: state={state:?} stack_free_min={stack} level_dbfs={level}");
                }
                self.cancel_local_if_stale();
                if matches!(self.session, Session::Owned { .. })
                    && self.status.lock().unwrap().state != State::Stopped
                    && Instant::now() >= self.poll_at
                {
                    self.poll_at = Instant::now() + Duration::from_secs(1);
                    if let Err(error) = self.poll() {
                        // Accepted cancellation can invalidate capacity during an observation.
                        // Retain URI evidence for its queued Stop instead of treating it as I/O loss.
                        if !self.cancel_local_if_stale() {
                            self.fail(error);
                        }
                    }
                }
                // A due retry waits for a LAN address; taking it leaves the session Idle.
                if let Some((lease, retry)) = self.network.lease().and_then(|lease| {
                    self.session
                        .due_retry(Instant::now())
                        .map(|retry| (lease, retry))
                }) {
                    match self
                        .resolve(&retry.player, lease)
                        .and_then(|group| self.connect(group, lease))
                    {
                        Ok(()) => {
                            // Reconnect observes status only. It never creates/reclaims an intent.
                            self.status.lock().unwrap().state = State::Stopped;
                        }
                        Err(error) => {
                            // connect() installs a client only on success, so the session is Idle.
                            self.status.lock().unwrap().last_error =
                                Some(format!("Reconnect: {error}"));
                            self.session = Session::retry_later(retry, Instant::now());
                        }
                    }
                }
            }
        }

        fn check_network(&mut self) {
            if self
                .session
                .lease()
                .is_some_and(|lease| !self.network.is_current(lease))
            {
                self.fail(anyhow!("Sonos network changed"));
            }
        }

        fn fail(&mut self, error: anyhow::Error) {
            self.queue.clear();
            self.policy
                .update(self.now(), protocol::AutoplayEvent::OwnershipLost);
            let selected = self.status.lock().unwrap().config.player_id.clone();
            self.poll_at = Instant::now();
            self.session =
                std::mem::replace(&mut self.session, Session::Idle).lost(selected, Instant::now());
            let mut status = self.status.lock().unwrap();
            status.active_player_id = None;
            status.state = State::Error;
            status.last_error = Some(error.to_string());
        }

        fn save(&mut self, config: SonosConfig) -> Result<()> {
            save_config(&self.status, config, |config| {
                self.nvs
                    .set_str("config", &serde_json::to_string(config)?)?;
                Ok(())
            })
        }

        fn command(&mut self, command: Command, lease: Option<Lease>) -> Result<()> {
            match command {
                Command::Discover => {
                    let previous = self.status.lock().unwrap().state;
                    self.status.lock().unwrap().state = State::Discovering;
                    let selected = self.status.lock().unwrap().config.player_id.clone();
                    let result = discover(
                        selected.as_deref(),
                        &self.network,
                        lease.context("Wi-Fi has no LAN address")?,
                    );
                    self.status.lock().unwrap().state = previous;
                    let groups = result?;
                    let mut status = self.status.lock().unwrap();
                    status.groups = groups.into_iter().filter(|g| g.is_leader).collect();
                    if !self.session.recovering() {
                        status.last_error = None;
                    }
                }
                Command::Select(player) => {
                    self.stop()?;
                    let mut config = self.status.lock().unwrap().config.clone();
                    config.player_id = Some(player);
                    self.save(config)?;
                }
                Command::SetAutoplay(enabled) => {
                    let mut config = self.status.lock().unwrap().config.clone();
                    config.autoplay = enabled;
                    self.save(config)?;
                    self.policy
                        .update(self.now(), protocol::AutoplayEvent::Enabled(enabled));
                }
                Command::SetEnabled(enabled) => {
                    // Turning Sonos off also turns autoplay off, so nothing starts by itself while it is off.
                    let mut config = self.status.lock().unwrap().config.clone();
                    config.enabled = enabled;
                    if !enabled {
                        config.autoplay = false;
                    }
                    self.save(config)?;
                    if !enabled {
                        self.policy
                            .update(self.now(), protocol::AutoplayEvent::Enabled(false));
                    }
                }
                Command::SetFormat(format) => {
                    let mut config = self.status.lock().unwrap().config.clone();
                    // `admit` is the gate; the stream runtime refuses MP3 again when it plays.
                    debug_assert!(
                        format != esp_shazam_core::stream_buffer::OutputFormat::Mp3
                            || self.status.lock().unwrap().buffer_available,
                        "MP3 admitted without the buffer"
                    );
                    config.output_format = format;
                    self.save(config)?;
                    if self.status.lock().unwrap().state == State::Stopped {
                        crate::stream::select_format(
                            format,
                            false,
                            self.commands
                                .token(self.generation)
                                .map_err(|error| anyhow!(error))?,
                        )?;
                    }
                }
                Command::SetBufferPreset(preset) => {
                    let mut config = self.status.lock().unwrap().config.clone();
                    debug_assert!(
                        self.status.lock().unwrap().buffer_available,
                        "buffer preset admitted without the buffer"
                    );
                    config.buffer_preset = preset;
                    self.save(config)?;
                }
                Command::Play => self.play(lease)?,
                Command::Stop => self.stop()?,
            }
            Ok(())
        }

        fn resolve(&mut self, player: &str, lease: Lease) -> Result<DiscoveryGroup> {
            let groups = discover(Some(player), &self.network, lease)?;
            let group = resolve_group(&groups, player);
            self.status.lock().unwrap().groups =
                groups.into_iter().filter(|g| g.is_leader).collect();
            group.ok_or_else(|| anyhow!("Selected player's current group leader is unavailable"))
        }

        fn connect(&mut self, group: DiscoveryGroup, lease: Lease) -> Result<()> {
            let client = with_network(&self.network, lease, || {
                Client::connect(&group, Instant::now() + TIMEOUT)
            })
            .map_err(|error| anyhow!(error))??;
            self.session = Session::Connected { client, lease };
            Ok(())
        }

        fn play(&mut self, lease: Option<Lease>) -> Result<()> {
            if !self.commands.is_current(self.generation) {
                return Ok(());
            }
            let lease = lease.context("Wi-Fi has no LAN address")?;
            let result = self.play_current(lease);
            if !self.commands.is_current(self.generation) {
                // Keep URI evidence for the queued explicit Stop; revoke delivery and later Play.
                self.cancel_local_if_stale();
                return Ok(());
            }
            result
        }

        fn play_current(&mut self, lease: Lease) -> Result<()> {
            with_network(&self.network, lease, || ()).map_err(|error| anyhow!(error))?;
            let play_token = self
                .commands
                .token(self.generation)
                .map_err(|error| anyhow!(error))?;
            self.stop()?;
            let player = self
                .status
                .lock()
                .unwrap()
                .config
                .player_id
                .clone()
                .context("Choose a Sonos destination first")?;
            let format = self.status.lock().unwrap().config.output_format;
            ensure!(
                self.commands.is_current(self.generation),
                "Sonos Play cancelled"
            );
            self.status.lock().unwrap().state = State::Connecting;
            let group = self.resolve(&player, lease)?;
            ensure!(
                self.commands.is_current(self.generation),
                "Sonos Play cancelled"
            );
            let buffer = {
                let status = self.status.lock().unwrap();
                status
                    .buffer_available
                    .then_some(status.config.buffer_preset)
            };
            let intent = self
                .queue
                .capacity
                .reserve(
                    group.host.into(),
                    self.network.clone(),
                    lease,
                    play_token.clone(),
                    format,
                    buffer,
                    Instant::now(),
                )
                .context("Sonos reservation unavailable")?;
            self.queue.snapshot.lock().unwrap().intent = Some(intent);
            let mut cleanup = IntentGuard {
                queue: self.queue.clone(),
                id: intent,
                armed: true,
            };
            log::info!(
                "stream-capacity: reserved sonos_intent={} network_generation={} held={}",
                intent,
                lease.generation,
                self.queue.capacity.held()
            );
            crate::stream::select_format(format, true, play_token.clone())?;
            self.status.lock().unwrap().active_format = crate::stream::active_format();
            let reclaim = Instant::now() + Duration::from_secs(12);
            while !self.queue.capacity.ready(intent) {
                ensure!(
                    self.queue.capacity.valid(intent),
                    "Sonos reservation expired or cancelled"
                );
                ensure!(
                    Instant::now() < reclaim,
                    "Sonos listener reclamation timed out"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            self.connect(group.clone(), lease)?;
            ensure!(
                self.queue.capacity.valid(intent),
                "Sonos reservation expired or cancelled"
            );
            let mut mac = [0u8; 6];
            sys::esp!(unsafe { sys::esp_efuse_mac_get_default(mac.as_mut_ptr()) })?;
            let context = format!(
                "esp-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            );
            self.sequence = self
                .sequence
                .checked_add(1)
                .context("Sonos intent counter exhausted")?;
            let item = format!("{context}-{}-{}", self.now(), self.sequence);
            let snapshot = queue_for_play(
                item,
                lease.address,
                &self.status.lock().unwrap(),
                lease.generation,
                intent,
            )
            .map_err(|error| anyhow!("Invalid Sonos queue: {error:?}"))?;
            let uri = sonos_queue::media_url(&snapshot);
            // Publish before SetURI: a lost response may still have committed the URI.
            self.commands
                .run_if_current(self.generation, || -> Result<()> {
                    ensure!(
                        self.queue.capacity.current_ready(intent),
                        "Sonos capacity unavailable"
                    );
                    with_network(&self.network, lease, || self.queue.publish(snapshot))
                        .map_err(|error| anyhow!(error))?;
                    Ok(())
                })
                .map_err(|error| anyhow!(error))??;
            self.session = std::mem::replace(&mut self.session, Session::Idle).own(uri.clone());
            self.status.lock().unwrap().active_player_id = Some(player);
            let Session::Owned { client, .. } = &self.session else {
                return Err(anyhow!("Sonos disconnected"));
            };
            with_network(&self.network, lease, || {
                client.verify(Instant::now() + TIMEOUT)
            })
            .map_err(|error| anyhow!(error))??;
            self.commands
                .run_if_current(self.generation, || -> Result<()> {
                    ensure!(
                        self.queue.capacity.current_ready(intent),
                        "Sonos capacity unavailable"
                    );
                    with_network(&self.network, lease, || {
                        client.set_uri(&uri, Instant::now() + TIMEOUT)
                    })
                    .map_err(|error| anyhow!(error))??;
                    Ok(())
                })
                .map_err(|error| anyhow!(error))??;
            let observed = with_network(&self.network, lease, || {
                client.media_uri(Instant::now() + TIMEOUT)
            })
            .map_err(|error| anyhow!(error))??;
            ensure!(observed == uri, "Playback source changed before Play");
            self.commands
                .run_if_current(self.generation, || -> Result<()> {
                    ensure!(
                        self.queue.play_current(intent),
                        "Sonos capacity unavailable"
                    );
                    with_network(&self.network, lease, || {
                        client.play(Instant::now() + TIMEOUT)
                    })
                    .map_err(|error| anyhow!(error))??;
                    Ok(())
                })
                .map_err(|error| anyhow!(error))??;
            self.policy
                .update(self.now(), protocol::AutoplayEvent::OwnershipAcquired);
            self.status.lock().unwrap().state = State::Buffering;
            self.status.lock().unwrap().last_error = None;
            cleanup.armed = false;
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            self.queue.clear();
            self.policy
                .update(self.now(), protocol::AutoplayEvent::ManualStop);
            // Stop ends every phase: a pending retry, a connected client, the owned URI and its recovery.
            let session = std::mem::replace(&mut self.session, Session::Idle);
            let lease = session.lease();
            let result = match session {
                Session::Owned {
                    client, lease, uri, ..
                } if self.network.is_current(lease) => (|| -> Result<()> {
                    with_network(&self.network, lease, || {
                        client.verify(Instant::now() + TIMEOUT)
                    })
                    .map_err(|error| anyhow!(error))??;
                    let current = with_network(&self.network, lease, || {
                        client.media_uri(Instant::now() + TIMEOUT)
                    })
                    .map_err(|error| anyhow!(error))??;
                    if HttpOwnership::new(uri).matches(&current) {
                        // SOAP has an unavoidable observation-to-write takeover race.
                        with_network(&self.network, lease, || {
                            client.stop(Instant::now() + TIMEOUT)
                        })
                        .map_err(|error| anyhow!(error))??;
                    }
                    Ok(())
                })(),
                // Nothing of ours plays remotely: no owned URI, or the network moved under it.
                Session::Owned { .. }
                | Session::Connected { .. }
                | Session::Reconnecting(_)
                | Session::Idle => Ok(()),
            };
            self.poll_at = Instant::now();
            let mut status = self.status.lock().unwrap();
            status.active_player_id = None;
            status.state = State::Stopped;
            status.last_error = None;
            stop_completion(&self.network, lease, result)
        }

        fn cancel_local_if_stale(&mut self) -> bool {
            let cancelled = {
                let mut status = self.status.lock().unwrap();
                let cancelled = revoke_cancelled_play(
                    &self.queue,
                    &self.commands,
                    self.generation,
                    &mut status.state,
                );
                if cancelled {
                    status.active_player_id = None;
                }
                cancelled
            };
            if cancelled {
                self.policy
                    .update(self.now(), protocol::AutoplayEvent::OwnershipLost);
                self.session = std::mem::replace(&mut self.session, Session::Idle).cancelled();
            }
            cancelled
        }

        fn poll(&mut self) -> Result<()> {
            if self.cancel_local_if_stale() {
                return Ok(());
            }
            let now = Instant::now();
            let Session::Owned {
                client,
                lease,
                uri,
                recovery,
            } = &mut self.session
            else {
                return Err(anyhow!("Sonos disconnected"));
            };
            let lease = *lease;
            let deadline = if let Some(recovery) = recovery.as_mut() {
                if now < recovery.next && now < recovery.deadline {
                    self.poll_at = recovery.next.min(recovery.deadline);
                    return Ok(());
                }
                recovery.begin(now)?
            } else {
                now + Duration::from_secs(5)
            };
            observation_checkpoint(
                &self.queue,
                &self.commands,
                self.generation,
                &self.network,
                lease,
            )?;
            if recovery.is_some() {
                commit_observation(
                    &self.queue,
                    &self.commands,
                    self.generation,
                    &self.network,
                    lease,
                    || {
                        self.status.lock().unwrap().state = State::Recovering;
                    },
                )?;
            }
            let observed = client.observe(uri, deadline, || {
                observation_checkpoint(
                    &self.queue,
                    &self.commands,
                    self.generation,
                    &self.network,
                    lease,
                )
            });
            let mut seed = 0u32;
            if recovery.is_none() && observed.as_ref().is_err_and(recoverable_observation_error) {
                // SAFETY: the SDK writes exactly the supplied seed's byte length.
                unsafe {
                    sys::esp_fill_random((&mut seed as *mut u32).cast(), std::mem::size_of::<u32>())
                };
            }
            let state = ControlObservation {
                queue: &self.queue,
                commands: &self.commands,
                generation: self.generation,
                lease,
                status: &self.status,
            }
            .finish(recovery, observed, Instant::now(), seed)?;
            if let Some(recovery) = recovery.as_ref() {
                self.poll_at = recovery.next.min(recovery.deadline);
            }
            if state == State::Stopped {
                self.session = std::mem::replace(&mut self.session, Session::Idle).released();
                self.policy
                    .update(self.now(), protocol::AutoplayEvent::OwnershipLost);
            }
            Ok(())
        }
    }
}
#[cfg(target_os = "espidf")]
pub use device::*;

#[cfg(test)]
fn reserve_test_queue(queue: &Queue, snapshot: &mut QueueSnapshot) {
    let id = queue
        .capacity
        .reserve(
            "192.0.2.20".parse().unwrap(),
            queue.network.clone(),
            queue.network.lease().unwrap(),
            std::sync::Arc::new(std::sync::Mutex::new(true)),
            snapshot.format,
            snapshot.buffer_preset,
            std::time::Instant::now(),
        )
        .unwrap();
    snapshot.sonos_intent = id;
    queue.snapshot.lock().unwrap().intent = Some(id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_buffer_updates_only_future_queue_and_failed_save_keeps_settings() {
        let status = std::sync::Mutex::new(Snapshot {
            config: SonosConfig {
                enabled: true,
                player_id: Some("selected".into()),
                autoplay: true,
                ..Default::default()
            },
            active_format: esp_shazam_core::stream_buffer::OutputFormat::Flac,
            buffer_available: true,
            groups: vec![],
            active_player_id: Some("selected".into()),
            state: State::Playing,
            last_error: None,
        });
        let host = "192.0.2.10".parse().unwrap();
        let current =
            queue_for_play("current".into(), host, &status.lock().unwrap(), 0, 41).unwrap();
        let mut config = status.lock().unwrap().config.clone();
        config.buffer_preset = esp_shazam_core::stream_buffer::BufferPreset::High;
        let saved = std::sync::Mutex::new(String::new());
        save_config(&status, config.clone(), |config| {
            *saved.lock().unwrap() = serde_json::to_string(config).unwrap();
            Ok::<_, &str>(())
        })
        .unwrap();
        assert_eq!(
            serde_json::from_str::<SonosConfig>(&saved.lock().unwrap()).unwrap(),
            config
        );
        assert_eq!(status.lock().unwrap().config, config);
        assert_eq!(status.lock().unwrap().state, State::Playing);
        assert_eq!(
            status.lock().unwrap().active_player_id.as_deref(),
            Some("selected")
        );
        assert_eq!(
            current.buffer_preset,
            Some(esp_shazam_core::stream_buffer::BufferPreset::Medium)
        );
        assert_eq!(
            queue_for_play("next".into(), host, &status.lock().unwrap(), 0, 41)
                .unwrap()
                .buffer_preset,
            Some(esp_shazam_core::stream_buffer::BufferPreset::High)
        );
        let mut failed = config.clone();
        failed.buffer_preset = esp_shazam_core::stream_buffer::BufferPreset::Low;
        assert_eq!(
            save_config(&status, failed, |_| Err("NVS failed")),
            Err("NVS failed")
        );
        assert_eq!(status.lock().unwrap().config, config);
        status.lock().unwrap().buffer_available = false;
        assert_eq!(
            queue_for_play("live".into(), host, &status.lock().unwrap(), 0, 41)
                .unwrap()
                .buffer_preset,
            None
        );
    }

    #[test]
    fn buffer_command_keeps_inflight_play_and_stop_cancels_it() {
        let (commands, receiver) = Commands::new(network_tests::ready_network());
        commands.submit(Command::Play).unwrap();
        let play = receiver.recv().unwrap();
        commands
            .submit(Command::SetBufferPreset(
                esp_shazam_core::stream_buffer::BufferPreset::High,
            ))
            .unwrap();
        assert!(commands.is_current(play.generation));
        commands.submit(Command::Stop).unwrap();
        assert!(!commands.is_current(play.generation));
    }

    #[test]
    fn admission_refuses_each_case_and_keeps_stop_and_enable_while_off() {
        use esp_shazam_core::stream_buffer::{BufferPreset, OutputFormat};
        let mut status = Snapshot {
            config: SonosConfig::default(),
            buffer_available: false,
            active_format: OutputFormat::Flac,
            groups: Vec::new(),
            active_player_id: None,
            state: State::Stopped,
            last_error: None,
        };
        // Off: only Stop and Enable pass, so an off controller can still be cancelled or turned on.
        for command in [
            Command::Discover,
            Command::Select("RINCON_TEST01400".into()),
            Command::Play,
            Command::SetAutoplay(true),
            Command::SetFormat(OutputFormat::Flac),
            Command::SetBufferPreset(BufferPreset::Low),
        ] {
            assert_eq!(
                admit(&command, &status),
                Err(Refusal::Disabled),
                "{command:?}"
            );
        }
        assert_eq!(admit(&Command::Stop, &status), Ok(()));
        assert_eq!(admit(&Command::SetEnabled(true), &status), Ok(()));
        assert_eq!(admit(&Command::SetEnabled(false), &status), Ok(()));
        status.config.enabled = true;
        assert_eq!(admit(&Command::Play, &status), Ok(()));
        assert_eq!(
            admit(&Command::SetBufferPreset(BufferPreset::High), &status),
            Err(Refusal::BufferUnavailable)
        );
        assert_eq!(
            admit(&Command::SetFormat(OutputFormat::Mp3), &status),
            Err(Refusal::Mp3Unavailable)
        );
        assert_eq!(
            admit(&Command::SetFormat(OutputFormat::Flac), &status),
            Ok(())
        );
        status.buffer_available = true;
        assert_eq!(
            admit(&Command::SetBufferPreset(BufferPreset::High), &status),
            Ok(())
        );
        assert_eq!(
            admit(&Command::SetFormat(OutputFormat::Mp3), &status),
            Ok(())
        );
        for player in ["", "bad\nidentity", "RINCON.test+id\u{7f}"] {
            assert_eq!(
                admit(&Command::Select(player.into()), &status),
                Err(Refusal::InvalidPlayer),
                "{player:?}"
            );
        }
        assert_eq!(
            admit(&Command::Select("RINCON_TEST01400".into()), &status),
            Ok(())
        );
    }

    #[test]
    fn command_queue_refuses_overflow_and_a_gone_worker() {
        let (commands, receiver) = Commands::new(network_tests::ready_network());
        for _ in 0..4 {
            commands.submit(Command::Play).unwrap();
        }
        assert_eq!(commands.submit(Command::Play), Err(Refusal::QueueFull));
        drop(receiver);
        assert_eq!(
            commands.submit(Command::Stop),
            Err(Refusal::WorkerUnavailable)
        );
    }

    #[test]
    fn failed_worker_launch_retains_receiver_and_saved_state_for_retry() {
        use std::io::{Read, Seek, SeekFrom, Write};
        use std::sync::mpsc;

        struct SavedPath(std::path::PathBuf);
        impl Drop for SavedPath {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = SavedPath(
            std::env::temp_dir().join(format!("sonos-launch-{}-{unique}.json", std::process::id())),
        );
        let mut saved = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path.0)
            .unwrap();
        saved.write_all(b"autoplay=false").unwrap();
        let (sender, receiver) = mpsc::channel();
        let start = WorkerStart::new((receiver, saved));
        let failed = start.ensure_started(|resources| {
            // Invalid stack size is rejected by the actual std thread implementation.
            std::thread::Builder::new()
                .stack_size(usize::MAX)
                .spawn(move || drop(resources))
                .map(|_| ())
        });
        assert!(failed.is_err());
        let (finished, result) = mpsc::channel();
        let launched = start.ensure_started(|resources| {
            std::thread::Builder::new()
                .spawn(move || {
                    let (receiver, mut saved) = resources.lock().unwrap().take().unwrap();
                    assert_eq!(receiver.recv().unwrap(), "play");
                    saved.seek(SeekFrom::Start(0)).unwrap();
                    let mut config = String::new();
                    saved.read_to_string(&mut config).unwrap();
                    assert_eq!(config, "autoplay=false");
                    saved.seek(SeekFrom::Start(0)).unwrap();
                    saved.write_all(b"autoplay=true ").unwrap();
                    finished.send(()).unwrap();
                })
                .map(|_| ())
        });
        assert!(launched.is_ok());
        assert!(
            sender.send("play").is_ok(),
            "sole receiver must survive failed launch"
        );
        result
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(std::fs::read(&path.0).unwrap(), b"autoplay=true ");
    }

    #[test]
    fn concurrent_launch_attempts_start_one_real_worker() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Barrier,
        };
        let count = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(WorkerStart::new(()));
        assert!(start
            .ensure_started(|resources| {
                std::thread::Builder::new()
                    .stack_size(usize::MAX)
                    .spawn(move || drop(resources))
                    .map(|_| ())
            })
            .is_err());
        let barrier = Arc::new(Barrier::new(8));
        let (finished, result) = std::sync::mpsc::channel();
        let callers: Vec<_> = (0..8)
            .map(|_| {
                let (count, start, barrier, finished) = (
                    count.clone(),
                    start.clone(),
                    barrier.clone(),
                    finished.clone(),
                );
                std::thread::spawn(move || {
                    barrier.wait();
                    start
                        .ensure_started(|resources| {
                            count.fetch_add(1, Ordering::SeqCst);
                            std::thread::Builder::new()
                                .spawn(move || {
                                    resources.lock().unwrap().take().unwrap();
                                    finished.send(()).unwrap();
                                })
                                .map(|_| ())
                        })
                        .unwrap();
                })
            })
            .collect();
        for caller in callers {
            caller.join().unwrap();
        }
        result
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn initial_autoplay_launch_failure_is_visible_and_keeps_manual_retry() {
        let status = std::sync::Mutex::new(Snapshot {
            active_format: esp_shazam_core::stream_buffer::OutputFormat::Flac,
            buffer_available: true,
            config: SonosConfig {
                enabled: true,
                autoplay: true,
                ..Default::default()
            },
            groups: Vec::new(),
            active_player_id: None,
            state: State::Stopped,
            last_error: None,
        });
        let (commands, receiver) = Commands::new(network_tests::ready_network());
        let start = WorkerStart::new(receiver);
        start_autoplay(&status, || {
            start.ensure_started(|resources| {
                std::thread::Builder::new()
                    .stack_size(usize::MAX)
                    .spawn(move || drop(resources))
                    .map(|_| ())
            })
        });
        let snapshot = status.lock().unwrap().clone();
        assert_eq!(snapshot.state, State::Error);
        assert!(snapshot
            .last_error
            .unwrap()
            .contains("Cannot start Sonos worker"));
        assert!(
            snapshot.config.autoplay,
            "launch failure must not change persisted autoplay"
        );
        let (finished, result) = std::sync::mpsc::channel();
        start
            .ensure_started(|resources| {
                std::thread::Builder::new()
                    .spawn(move || {
                        let receiver = resources.lock().unwrap().take().unwrap();
                        assert!(matches!(receiver.recv().unwrap().command, Command::Stop));
                        finished.send(()).unwrap();
                    })
                    .map(|_| ())
            })
            .unwrap();
        commands.submit(Command::Stop).unwrap();
        result
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
    }

    #[test]
    fn disabled_autoplay_never_attempts_initial_worker_launch() {
        // A config saved before the Sonos opt-in may carry autoplay with control off.
        for config in [
            SonosConfig::default(),
            SonosConfig {
                autoplay: true,
                ..Default::default()
            },
        ] {
            let status = std::sync::Mutex::new(Snapshot {
                active_format: esp_shazam_core::stream_buffer::OutputFormat::Flac,
                buffer_available: true,
                config,
                groups: Vec::new(),
                active_player_id: None,
                state: State::Stopped,
                last_error: Some("saved config error".into()),
            });
            start_autoplay(&status, || -> Result<(), std::io::Error> {
                panic!("disabled autoplay must stay lazy");
            });
            let snapshot = status.lock().unwrap();
            assert_eq!(snapshot.state, State::Stopped);
            assert_eq!(snapshot.last_error.as_deref(), Some("saved config error"));
        }
    }

    #[test]
    fn commands_cancel_queue_protection_and_old_startup_cleanup_cannot_clear_retry() {
        use esp_shazam_core::{stream_buffer::OutputFormat, stream_wait::Slot};
        use std::time::Instant;
        let network = network_tests::ready_network();
        let lease = network.lease().unwrap();
        let (commands, _receiver) = Commands::new(network.clone());
        let queue = Queue::new(network.clone());
        let token = commands.token(0).unwrap();
        let id = queue
            .capacity
            .reserve(
                "192.0.2.20".parse().unwrap(),
                network.clone(),
                lease,
                token,
                OutputFormat::Flac,
                None,
                Instant::now(),
            )
            .unwrap();
        queue.snapshot.lock().unwrap().intent = Some(id);
        let old_cleanup = IntentGuard {
            queue: queue.clone(),
            id,
            armed: true,
        };
        let snapshot = QueueSnapshot::new(
            "owned".into(),
            lease.address,
            None,
            OutputFormat::Flac,
            lease.generation,
            id,
        )
        .unwrap();
        commands
            .run_if_current(0, || {
                assert!(queue.intent_current());
                queue.publish(snapshot);
            })
            .unwrap();
        assert!(queue.snapshot().is_some());
        commands.submit(Command::Stop).unwrap();
        assert!(queue.snapshot().is_none());
        assert!(!queue.intent_valid());
        assert!(commands
            .run_if_current(0, || panic!("cancelled Play published"))
            .is_err());
        let next = queue
            .capacity
            .reserve(
                "192.0.2.20".parse().unwrap(),
                network,
                lease,
                commands.token(1).unwrap(),
                OutputFormat::Flac,
                None,
                Instant::now(),
            )
            .unwrap();
        queue.snapshot.lock().unwrap().intent = Some(next);
        drop(old_cleanup);
        assert!(queue.intent_valid());
        commands
            .run_if_current(1, || {
                assert!(queue.intent_current());
                queue.publish(
                    QueueSnapshot::new(
                        "retry".into(),
                        lease.address,
                        None,
                        OutputFormat::Flac,
                        lease.generation,
                        next,
                    )
                    .unwrap(),
                );
            })
            .unwrap();
        assert_eq!(queue.snapshot().unwrap().item_id, "retry");
        // Startup failure uses the same ID-bound guard and returns both diagnostic places.
        drop(IntentGuard {
            queue: queue.clone(),
            id: next,
            armed: true,
        });
        assert!(queue.snapshot().is_none());
        let first = Slot::take(&queue.capacity, 2).unwrap();
        let second = Slot::take(&queue.capacity, 2).unwrap();
        assert!(Slot::take(&queue.capacity, 2).is_none());
        drop((first, second));
    }

    #[test]
    fn accepted_stop_disable_and_selection_cancel_earlier_play_but_not_later_play() {
        for cancel in [
            Command::Stop,
            Command::SetAutoplay(false),
            Command::Select("other".into()),
        ] {
            let (commands, receiver) = Commands::new(network_tests::ready_network());
            commands.submit(Command::Play).unwrap();
            let playing = receiver.recv().unwrap();
            assert!(commands.is_current(playing.generation));
            commands.submit(cancel).unwrap();
            assert!(!commands.is_current(playing.generation));
            commands.submit(Command::Play).unwrap();
            let _cancel = receiver.recv().unwrap();
            let next = receiver.recv().unwrap();
            assert!(matches!(next.command, Command::Play));
            assert!(commands.is_current(next.generation));
        }
    }

    #[test]
    fn rejected_cancel_and_unrelated_commands_do_not_cancel_play() {
        let (commands, receiver) = Commands::new(network_tests::ready_network());
        commands.submit(Command::Play).unwrap();
        let playing = receiver.recv().unwrap();
        for _ in 0..4 {
            commands.submit(Command::Discover).unwrap();
        }
        assert!(commands.submit(Command::Stop).is_err());
        assert!(commands.is_current(playing.generation));
        let _ = receiver.recv().unwrap();
        commands.submit(Command::SetAutoplay(true)).unwrap();
        assert!(commands.is_current(playing.generation));
    }

    #[test]
    fn repeated_loud_sample_does_not_complete_active_dwell() {
        use std::time::{Duration, Instant};
        let epoch = Instant::now();
        let mut samples = LevelSamples::default();
        let mut policy = protocol::Autoplay::new(-50.0, -60.0).unwrap();
        policy.update(0, protocol::AutoplayEvent::Enabled(true));
        for ms in (0..=10000).step_by(100) {
            let measured = epoch + Duration::from_millis(ms);
            assert_eq!(
                samples.update(&mut policy, epoch, Some((-85.0, measured)), measured),
                protocol::Action::None
            );
        }
        let loud = epoch + Duration::from_millis(10100);
        for ms in (10100..=12500).step_by(100) {
            assert_eq!(
                samples.update(
                    &mut policy,
                    epoch,
                    Some((-43.0, loud)),
                    epoch + Duration::from_millis(ms)
                ),
                protocol::Action::None
            );
        }
    }

    #[test]
    fn fresh_measurements_complete_dwell_using_capture_time() {
        use std::time::{Duration, Instant};
        let epoch = Instant::now();
        let mut samples = LevelSamples::default();
        let mut policy = protocol::Autoplay::new(-50.0, -60.0).unwrap();
        policy.update(0, protocol::AutoplayEvent::Enabled(true));
        for ms in (0..=10000).step_by(100) {
            let measured = epoch + Duration::from_millis(ms);
            samples.update(&mut policy, epoch, Some((-85.0, measured)), measured);
        }
        for (measured_ms, polled_ms, expected) in [
            (10100, 10100, protocol::Action::None),
            (10200, 11000, protocol::Action::None),
            (10600, 11100, protocol::Action::Start),
        ] {
            assert_eq!(
                samples.update(
                    &mut policy,
                    epoch,
                    Some((-43.0, epoch + Duration::from_millis(measured_ms))),
                    epoch + Duration::from_millis(polled_ms)
                ),
                expected
            );
        }
    }

    #[test]
    fn fresh_samples_are_usable_but_stale_missing_and_future_samples_are_not() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        assert_eq!(sample_level(Some((-43.0, now)), now), -43.0);
        assert_eq!(
            sample_level(Some((-85.0, now - Duration::from_secs(2))), now),
            -85.0
        );
        assert!(sample_level(Some((-85.0, now - Duration::from_millis(2001))), now).is_nan());
        assert!(sample_level(Some((-43.0, now + Duration::from_secs(1))), now).is_nan());
        assert!(sample_level(None, now).is_nan());
        assert!(sample_level(Some((f32::INFINITY, now)), now).is_nan());
    }

    #[test]
    fn stale_silence_cannot_arm_subsequent_audio() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let mut policy = protocol::Autoplay::new(-50.0, -60.0).unwrap();
        policy.update(0, protocol::AutoplayEvent::Enabled(true));
        for ms in (0..=11000).step_by(100) {
            let level = sample_level(Some((-85.0, now)), now + Duration::from_millis(ms));
            assert_eq!(
                policy.update(ms, protocol::AutoplayEvent::Level(level)),
                protocol::Action::None
            );
        }
        for ms in (12000..=15000).step_by(100) {
            let level = sample_level(
                Some((-43.0, now + Duration::from_secs(12))),
                now + Duration::from_millis(ms),
            );
            assert_eq!(
                policy.update(ms, protocol::AutoplayEvent::Level(level)),
                protocol::Action::None
            );
        }
    }

    #[test]
    fn persisted_player_resolves_current_leader_after_regrouping() {
        let follower = DiscoveryGroup {
            player_id: "selected".into(),
            group_id: "new-group".into(),
            household_id: "house".into(),
            name: "Room".into(),
            host: "192.0.2.10".parse().unwrap(),
            is_leader: false,
        };
        let leader = DiscoveryGroup {
            player_id: "leader".into(),
            host: "192.0.2.11".parse().unwrap(),
            is_leader: true,
            ..follower.clone()
        };
        let other_house = DiscoveryGroup {
            player_id: "wrong".into(),
            household_id: "other-house".into(),
            ..leader.clone()
        };
        assert_eq!(
            resolve_group(&[follower.clone(), other_house, leader.clone()], "selected"),
            Some(leader)
        );
        assert_eq!(resolve_group(&[follower], "selected"), None);
    }

    #[test]
    fn damaged_saved_config_is_visible_and_recoverable_without_starting_playback() {
        for saved in [
            "not-json",
            r#"{"player_id":"","autoplay":true}"#,
            r#"{"player_id":"bad\nidentity","autoplay":true}"#,
        ] {
            let (config, error) = decode_config(Some(saved));
            assert_eq!(config, SonosConfig::default());
            assert!(error.unwrap().contains("Invalid saved Sonos configuration"));
        }
        // A config saved before Sonos became opt-in has no `enabled` field and comes up off.
        let (older, error) = decode_config(Some(r#"{"player_id":"player","autoplay":true}"#));
        assert!(!older.enabled);
        assert!(error.is_none());
        let (config, error) = decode_config(Some(r#"{"player_id":"player","autoplay":true}"#));
        assert_eq!(config.player_id.as_deref(), Some("player"));
        assert!(config.autoplay);
        assert_eq!(error, None);
        assert_eq!(decode_config(None), (SonosConfig::default(), None));
    }

    #[test]
    fn queue_publication_and_cleanup_share_one_snapshot_across_handles() {
        let queue = Queue::new(network_tests::ready_network());
        let reader = queue.clone();
        assert_eq!(reader.snapshot(), None);
        let mut snapshot = QueueSnapshot {
            item_id: "item-first".into(),
            host: "192.0.2.10".parse().unwrap(),
            network_generation: 1,
            sonos_intent: 41,
            buffer_preset: None,
            format: esp_shazam_core::stream_buffer::OutputFormat::Flac,
        };
        reserve_test_queue(&queue, &mut snapshot);
        queue.publish(snapshot.clone());
        assert_eq!(reader.snapshot(), Some(snapshot.clone()));
        let next = QueueSnapshot {
            item_id: "item-next".into(),
            ..snapshot
        };
        queue.publish(next.clone());
        assert_eq!(reader.snapshot(), Some(next));
        queue.clear();
        assert_eq!(reader.snapshot(), None);
    }
}

#[cfg(test)]
mod format_cancellation_tests {
    use super::*;
    #[test]
    fn stop_revokes_captured_token_and_old_play_cannot_borrow_replacement() {
        let (commands, receiver) = Commands::new(network_tests::ready_network());
        commands.submit(Command::Play).unwrap();
        let play = receiver.recv().unwrap();
        let token = commands.token(play.generation).unwrap();
        commands.submit(Command::Stop).unwrap();
        assert!(!*token.lock().unwrap());
        assert!(commands.token(play.generation).is_err());
        let stop = receiver.recv().unwrap();
        assert!(*commands.token(stop.generation).unwrap().lock().unwrap());
        let command = Command::SetFormat(esp_shazam_core::stream_buffer::OutputFormat::Mp3);
        assert!(
            matches!(command, Command::SetFormat(format) if format == esp_shazam_core::stream_buffer::OutputFormat::Mp3)
        );
    }
}

#[cfg(test)]
mod send_cancellation_tests {
    use super::*;
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };
    #[test]
    fn revoked_generation_cannot_start_final_operation() {
        let (commands, receiver) = Commands::new(network_tests::ready_network());
        commands.submit(Command::Play).unwrap();
        let play = receiver.recv().unwrap();
        commands.submit(Command::Stop).unwrap();
        let (sent, received) = mpsc::channel();
        assert!(commands
            .run_if_current(play.generation, || sent.send(()).unwrap())
            .is_err());
        assert!(received.try_recv().is_err());
    }
    #[test]
    fn cancellation_after_real_set_uri_send_prevents_play_send() {
        use std::{
            io::{Read, Write},
            net::{TcpListener, TcpStream},
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_millis(30)))
            .unwrap();
        let (commands, receiver) = Commands::new(network_tests::ready_network());
        commands.submit(Command::Play).unwrap();
        let play = receiver.recv().unwrap();
        commands
            .run_if_current(play.generation, || client.write_all(b"SetURI"))
            .unwrap()
            .unwrap();
        commands.submit(Command::Stop).unwrap();
        assert!(commands
            .run_if_current(play.generation, || client.write_all(b"Play"))
            .is_err());
        let mut bytes = [0; 6];
        peer.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"SetURI");
        assert!(peer.read(&mut [0]).is_err());
    }

    #[test]
    fn admitted_sonos_can_play_with_debug_filling_other_slot() {
        use esp_shazam_core::stream_buffer::OutputFormat;
        use esp_shazam_core::stream_wait::{Capacity, Slot};
        let network = network_tests::ready_network();
        let (commands, receiver) = Commands::new(network.clone());
        commands.submit(Command::Play).unwrap();
        let play = receiver.recv().unwrap();
        let queue = Queue::new(network.clone());
        let _debug = Slot::take(&queue.capacity, 2).unwrap();
        let lease = network.lease().unwrap();
        let peer = "192.0.2.20".parse().unwrap();
        let intent = queue
            .capacity
            .reserve(
                peer,
                network,
                lease,
                commands.token(play.generation).unwrap(),
                OutputFormat::Mp3,
                None,
                std::time::Instant::now(),
            )
            .unwrap();
        assert!(commands
            .run_if_current(play.generation, || queue.capacity.current_ready(intent))
            .unwrap());
        let _sonos = Capacity::admit(
            &queue.capacity,
            intent,
            peer,
            lease.generation,
            OutputFormat::Mp3,
            None,
        )
        .unwrap();
        assert_eq!(queue.capacity.held(), 2);
        assert!(!queue.capacity.current_ready(intent));
        assert!(commands
            .run_if_current(play.generation, || queue.play_current(intent))
            .unwrap());
        assert!(Slot::take(&queue.capacity, 2).is_none());
        commands.submit(Command::Stop).unwrap();
        assert!(commands
            .run_if_current(play.generation, || queue.play_current(intent))
            .is_err());
        assert!(!queue.capacity.valid(intent));
    }

    #[test]
    fn cancellation_releases_admission_and_stops_local_status() {
        use esp_shazam_core::stream_buffer::OutputFormat;
        use esp_shazam_core::stream_wait::Capacity;
        for cancel in [Command::Stop, Command::SetAutoplay(false)] {
            let network = network_tests::ready_network();
            let (commands, receiver) = Commands::new(network.clone());
            commands.submit(Command::Play).unwrap();
            let play = receiver.recv().unwrap();
            let queue = Queue::new(network.clone());
            let lease = network.lease().unwrap();
            let peer = "192.0.2.20".parse().unwrap();
            let intent = queue
                .capacity
                .reserve(
                    peer,
                    network,
                    lease,
                    commands.token(play.generation).unwrap(),
                    OutputFormat::Mp3,
                    None,
                    std::time::Instant::now(),
                )
                .unwrap();
            queue.snapshot.lock().unwrap().intent = Some(intent);
            let slot = Capacity::admit(
                &queue.capacity,
                intent,
                peer,
                lease.generation,
                OutputFormat::Mp3,
                None,
            )
            .unwrap();
            commands.submit(cancel).unwrap();
            let mut state = State::Playing;
            assert!(revoke_cancelled_play(
                &queue,
                &commands,
                play.generation,
                &mut state
            ));
            assert_eq!(state, State::Stopped);
            assert!(!queue.capacity.valid(intent));
            assert!(commands
                .run_if_current(play.generation, || panic!("late Play"))
                .is_err());
            assert_eq!(queue.snapshot.lock().unwrap().intent, None);
            drop(slot);
            assert_eq!(queue.capacity.held(), 0);
        }
    }

    #[test]
    fn stop_acceptance_waits_for_already_started_operation() {
        let (commands, receiver) = Commands::new(network_tests::ready_network());
        let commands = Arc::new(commands);
        commands.submit(Command::Play).unwrap();
        let play = receiver.recv().unwrap();
        let sending = commands.clone();
        let (started, running) = mpsc::channel();
        let (finish, finishing) = mpsc::channel();
        let operation = std::thread::spawn(move || {
            sending.run_if_current(play.generation, || {
                started.send(()).unwrap();
                finishing.recv_timeout(Duration::from_secs(1)).unwrap();
            })
        });
        running.recv_timeout(Duration::from_secs(1)).unwrap();
        let stopping = commands.clone();
        let (accepted, acceptance) = mpsc::channel();
        let stop = std::thread::spawn(move || {
            stopping.submit(Command::Stop).unwrap();
            accepted.send(()).unwrap();
        });
        assert!(acceptance.recv_timeout(Duration::from_millis(20)).is_err());
        finish.send(()).unwrap();
        operation.join().unwrap().unwrap();
        acceptance.recv_timeout(Duration::from_secs(1)).unwrap();
        stop.join().unwrap();
        assert!(commands.run_if_current(play.generation, || ()).is_err());
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;
    use esp_shazam_core::network_lifecycle::Network;

    pub(super) fn ready_network() -> Network {
        let network = Network::new();
        network.observe(network.snapshot(), Some("192.0.2.10".parse().unwrap()));
        network
    }

    #[test]
    fn queued_network_work_expires_even_when_same_address_returns() {
        let network = ready_network();
        let (commands, receiver) = Commands::new(network.clone());
        commands.submit(Command::Play).unwrap();
        commands.submit(Command::Discover).unwrap();
        let play = receiver.recv().unwrap();
        let discover = receiver.recv().unwrap();
        assert!(play.network_current(&network));
        assert!(discover.network_current(&network));
        network.invalidate();
        network.observe(network.snapshot(), Some("192.0.2.10".parse().unwrap()));
        assert!(!play.network_current(&network));
        assert!(!discover.network_current(&network));
        commands.submit(Command::Play).unwrap();
        assert!(receiver.recv().unwrap().network_current(&network));
    }

    #[test]
    fn offline_play_stays_stale_but_stop_and_settings_survive_loss() {
        let network = Network::new();
        let (commands, receiver) = Commands::new(network.clone());
        commands.submit(Command::Play).unwrap();
        network.observe(network.snapshot(), Some("192.0.2.10".parse().unwrap()));
        assert!(!receiver.recv().unwrap().network_current(&network));
        commands.submit(Command::Stop).unwrap();
        commands.submit(Command::SetAutoplay(false)).unwrap();
        network.invalidate();
        assert!(receiver.recv().unwrap().network_current(&network));
        assert!(receiver.recv().unwrap().network_current(&network));
    }

    #[test]
    fn network_checkpoint_rejects_real_reply_after_invalidation_without_locking_io() {
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};
        let network = ready_network();
        let lease = network.lease().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let invalidating = network.clone();
        let server = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let mut request = [0];
            peer.read_exact(&mut request).unwrap();
            invalidating.invalidate();
            peer.write_all(b"reply").unwrap();
        });
        let reply = with_network(&network, lease, || {
            client.write_all(b"x").unwrap();
            let mut reply = [0; 5];
            client.read_exact(&mut reply).unwrap();
            reply
        });
        assert_eq!(reply, Err("Sonos network changed"));
        server.join().unwrap();
        assert_eq!(
            with_network(&network, lease, || panic!("stale operation started")),
            Err::<(), _>("Sonos network changed")
        );
    }

    #[test]
    fn local_stop_completes_when_suspend_loses_its_network() {
        let network = ready_network();
        let lease = network.lease().unwrap();
        let suspended = with_network(&network, lease, || network.invalidate()).map(|_| ());
        assert!(suspended.is_err());
        assert_eq!(stop_completion(&network, Some(lease), suspended), Ok(()));
        let local = with_network(&network, lease, || ());
        assert!(stop_completion(&network, None, local).is_err());
        network.observe(network.snapshot(), Some(lease.address));
        let current = network.lease().unwrap();
        let failed: Result<(), &str> = Err("current transport failed");
        assert_eq!(
            stop_completion(&network, Some(current), failed),
            Err("current transport failed")
        );
    }

    #[test]
    fn queue_hides_old_urls_before_worker_cleanup_and_after_same_ip_rejoin() {
        let network = ready_network();
        let lease = network.lease().unwrap();
        let queue = Queue::new(network.clone());
        queue.publish({
            let mut snapshot = QueueSnapshot::new(
                "owned".into(),
                lease.address,
                None,
                esp_shazam_core::stream_buffer::OutputFormat::Flac,
                lease.generation,
                41,
            )
            .unwrap();
            reserve_test_queue(&queue, &mut snapshot);
            snapshot
        });
        assert!(queue.snapshot().is_some());
        network.invalidate();
        assert!(queue.snapshot().is_none());
        network.observe(network.snapshot(), Some(lease.address));
        assert!(queue.snapshot().is_none());
        let fresh = network.lease().unwrap();
        queue.publish({
            let mut snapshot = QueueSnapshot::new(
                "fresh".into(),
                fresh.address,
                None,
                esp_shazam_core::stream_buffer::OutputFormat::Flac,
                fresh.generation,
                41,
            )
            .unwrap();
            reserve_test_queue(&queue, &mut snapshot);
            snapshot
        });
        assert_eq!(queue.snapshot().unwrap().item_id, "fresh");
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn lease() -> Lease {
        network_tests::ready_network().lease().unwrap()
    }

    fn connected() -> Session<()> {
        Session::Connected {
            client: (),
            lease: lease(),
        }
    }

    fn owned(recovery: Option<ControlRecovery>) -> Session<()> {
        Session::Owned {
            client: (),
            lease: lease(),
            uri: "http://board/stream.mp3?sonos_intent=1".into(),
            recovery,
        }
    }

    #[test]
    fn lost_session_retries_the_selected_player_and_rests_otherwise() {
        let now = Instant::now();
        assert_eq!(
            connected().lost(Some("player".into()), now),
            Session::Reconnecting(Retry {
                player: "player".into(),
                at: now + Duration::from_secs(1),
                attempt: 0,
            })
        );
        assert_eq!(connected().lost(None, now), Session::Idle);
        assert!(matches!(
            owned(Some(ControlRecovery::new(now, 1))).lost(Some("player".into()), now),
            Session::Reconnecting(_)
        ));
        let pending = Session::<()>::Reconnecting(Retry {
            player: "player".into(),
            at: now,
            attempt: 1,
        });
        assert_eq!(pending.lost(Some("player".into()), now), Session::Idle);
        assert_eq!(
            Session::<()>::Idle.lost(Some("player".into()), now),
            Session::Idle
        );
    }

    #[test]
    fn reconnect_backoff_doubles_and_gives_up_after_three_attempts() {
        let now = Instant::now();
        let first = Retry {
            player: "player".into(),
            at: now,
            attempt: 0,
        };
        let Session::<()>::Reconnecting(second) = Session::retry_later(first, now) else {
            panic!("second attempt must be armed");
        };
        assert_eq!(
            (second.attempt, second.at),
            (1, now + Duration::from_secs(2))
        );
        let Session::<()>::Reconnecting(third) = Session::retry_later(second, now) else {
            panic!("third attempt must be armed");
        };
        assert_eq!((third.attempt, third.at), (2, now + Duration::from_secs(4)));
        assert_eq!(Session::<()>::retry_later(third, now), Session::Idle);
    }

    #[test]
    fn cancellation_drops_a_stale_retry_but_keeps_the_client() {
        let now = Instant::now();
        let pending = Session::<()>::Reconnecting(Retry {
            player: "player".into(),
            at: now,
            attempt: 0,
        });
        assert_eq!(pending.cancelled(), Session::Idle);
        assert_eq!(connected().cancelled(), connected());
        assert_eq!(Session::<()>::Idle.cancelled(), Session::Idle);
        // The owned URI stays for the queued checked Stop; only the read-only retry budget ends.
        assert_eq!(
            owned(Some(ControlRecovery::new(now, 1))).cancelled(),
            owned(None)
        );
    }

    #[test]
    fn owning_needs_a_live_client_and_release_keeps_it() {
        let now = Instant::now();
        let uri = String::from("http://board/stream.mp3?sonos_intent=1");
        assert_eq!(connected().own(uri.clone()), owned(None));
        // A fresh queue replaces the URI evidence and any recovery of the old one.
        assert_eq!(
            owned(Some(ControlRecovery::new(now, 1))).own("http://board/next".into()),
            Session::Owned {
                client: (),
                lease: lease(),
                uri: "http://board/next".into(),
                recovery: None,
            }
        );
        assert_eq!(Session::<()>::Idle.own(uri.clone()), Session::Idle);
        let pending = || {
            Session::<()>::Reconnecting(Retry {
                player: "player".into(),
                at: now,
                attempt: 0,
            })
        };
        assert_eq!(pending().own(uri), pending());
        assert_eq!(
            owned(Some(ControlRecovery::new(now, 1))).released(),
            connected()
        );
        assert_eq!(connected().released(), connected());
        assert_eq!(Session::<()>::Idle.released(), Session::Idle);
    }

    #[test]
    fn recovery_is_in_flight_only_while_an_owned_session_holds_a_budget() {
        let now = Instant::now();
        assert!(owned(Some(ControlRecovery::new(now, 1))).recovering());
        assert!(!owned(None).recovering());
        assert!(!connected().recovering());
        assert!(!Session::<()>::Idle.recovering());
    }

    #[test]
    fn a_due_retry_is_taken_once_and_other_phases_stay_put() {
        let now = Instant::now();
        let mut session = Session::<()>::Reconnecting(Retry {
            player: "player".into(),
            at: now + Duration::from_secs(1),
            attempt: 0,
        });
        assert!(session.due_retry(now).is_none());
        assert!(matches!(session, Session::Reconnecting(_)));
        let retry = session
            .due_retry(now + Duration::from_secs(1))
            .expect("due retry");
        assert_eq!(retry.player, "player");
        assert_eq!(session, Session::Idle);
        let mut session = connected();
        assert!(session.due_retry(now).is_none());
        assert_eq!(session, connected());
    }

    #[test]
    fn only_a_connected_session_has_a_lease() {
        let now = Instant::now();
        assert_eq!(connected().lease(), Some(lease()));
        assert_eq!(owned(None).lease(), Some(lease()));
        assert_eq!(Session::<()>::Idle.lease(), None);
        assert_eq!(
            Session::<()>::Reconnecting(Retry {
                player: "player".into(),
                at: now,
                attempt: 0,
            })
            .lease(),
            None
        );
    }
}

#[cfg(test)]
mod control_recovery_tests {
    use super::*;
    use std::{
        io,
        time::{Duration, Instant},
    };

    fn recovery_status() -> std::sync::Mutex<Snapshot> {
        std::sync::Mutex::new(Snapshot {
            config: SonosConfig::default(),
            buffer_available: true,
            active_format: esp_shazam_core::stream_buffer::OutputFormat::Flac,
            groups: vec![],
            active_player_id: Some("same-player".into()),
            state: State::Playing,
            last_error: None,
        })
    }

    fn with_observation(test: impl FnOnce(ControlObservation<'_>, QueueSnapshot)) {
        use esp_shazam_core::stream_buffer::OutputFormat;
        let network = network_tests::ready_network();
        let lease = network.lease().unwrap();
        let (commands, receiver) = Commands::new(network.clone());
        commands.submit(Command::Play).unwrap();
        let generation = receiver.recv().unwrap().generation;
        let queue = Queue::new(network.clone());
        let intent = queue
            .capacity
            .reserve(
                "192.0.2.20".parse().unwrap(),
                network,
                lease,
                commands.token(generation).unwrap(),
                OutputFormat::Flac,
                Some(esp_shazam_core::stream_buffer::BufferPreset::High),
                Instant::now(),
            )
            .unwrap();
        queue.snapshot.lock().unwrap().intent = Some(intent);
        let original = QueueSnapshot::new(
            "same-item".into(),
            lease.address,
            Some(esp_shazam_core::stream_buffer::BufferPreset::High),
            OutputFormat::Flac,
            lease.generation,
            intent,
        )
        .unwrap();
        queue.publish(original.clone());
        let status = recovery_status();
        test(
            ControlObservation {
                queue: &queue,
                commands: &commands,
                generation,
                lease,
                status: &status,
            },
            original,
        );
    }

    #[test]
    fn production_observation_retains_until_playing_and_cleans_terminal_replies() {
        use esp_shazam_core::sonos_upnp::TransportState;
        for terminal in 0..6 {
            with_observation(|observation, original| {
                let now = Instant::now();
                let mut recovery = None;
                assert_eq!(
                    observation
                        .finish(
                            &mut recovery,
                            Err(io::Error::from(io::ErrorKind::TimedOut)),
                            now,
                            17
                        )
                        .unwrap(),
                    State::Degraded
                );
                assert_eq!(observation.queue.snapshot(), Some(original.clone()));
                let retry = recovery.as_mut().unwrap();
                let next = retry.next;
                retry.begin(next).unwrap();
                assert_eq!(
                    observation
                        .finish(
                            &mut recovery,
                            Ok(Some(TransportState::Transitioning)),
                            next,
                            17
                        )
                        .unwrap(),
                    State::Degraded
                );
                assert_eq!(observation.queue.snapshot(), Some(original.clone()));
                assert!(observation.status.lock().unwrap().last_error.is_some());
                let outcome = match terminal {
                    0 => Ok(Some(TransportState::Playing)),
                    1 => Ok(Some(TransportState::Stopped)),
                    2 => Ok(Some(TransportState::Paused)),
                    3 => Ok(None),
                    4 => Err(io::Error::from(io::ErrorKind::InvalidData)),
                    _ => Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
                };
                let result = observation.finish(&mut recovery, outcome, next, 17);
                if terminal == 0 {
                    assert_eq!(result.unwrap(), State::Playing);
                    assert_eq!(observation.queue.snapshot(), Some(original));
                    assert_eq!(observation.status.lock().unwrap().last_error, None);
                } else {
                    assert_eq!(observation.queue.snapshot(), None);
                    assert!(!observation.queue.capacity.valid(original.sonos_intent));
                }
                assert!(recovery.is_none());
            });
        }
    }

    #[test]
    fn late_success_and_attempt_exhaustion_clear_delivery() {
        use esp_shazam_core::sonos_upnp::TransportState;
        for late in [false, true] {
            with_observation(|observation, original| {
                let now = Instant::now();
                let mut recovery = Some(ControlRecovery::new(now, 9));
                let mut next = now;
                for attempt in 0..3 {
                    next = recovery.as_ref().unwrap().next;
                    recovery.as_mut().unwrap().begin(next).unwrap();
                    if attempt < 2 {
                        observation
                            .finish(
                                &mut recovery,
                                Err(io::Error::from(io::ErrorKind::TimedOut)),
                                next,
                                9,
                            )
                            .unwrap();
                    }
                }
                let result = if late {
                    Ok(Some(TransportState::Playing))
                } else {
                    Ok(Some(TransportState::Transitioning))
                };
                let finished = if late {
                    now + Duration::from_secs(30)
                } else {
                    next
                };
                assert!(observation
                    .finish(&mut recovery, result, finished, 9)
                    .is_err());
                assert!(!observation.queue.capacity.valid(original.sonos_intent));
                assert!(recovery.is_none());
            });
        }
    }

    #[test]
    fn accepted_cancellations_and_network_loss_reject_late_success() {
        use esp_shazam_core::sonos_upnp::TransportState;
        for cancel in [
            Some(Command::Stop),
            Some(Command::Select("other".into())),
            Some(Command::SetAutoplay(false)),
            None,
        ] {
            with_observation(|observation, original| {
                let mut recovery = None;
                observation
                    .finish(
                        &mut recovery,
                        Err(io::Error::from(io::ErrorKind::TimedOut)),
                        Instant::now(),
                        3,
                    )
                    .unwrap();
                if let Some(command) = cancel {
                    observation.commands.submit(command).unwrap();
                } else {
                    observation.queue.network.invalidate();
                }
                assert!(observation
                    .finish(
                        &mut recovery,
                        Ok(Some(TransportState::Playing)),
                        Instant::now(),
                        3
                    )
                    .is_err());
                assert_eq!(observation.queue.snapshot(), None);
                assert!(!observation.queue.capacity.valid(original.sonos_intent));
                assert_eq!(observation.status.lock().unwrap().state, State::Degraded);
            });
        }
    }

    #[test]
    fn recovery_keeps_one_finite_budget_and_three_jittered_attempts() {
        let now = Instant::now();
        let mut recovery = ControlRecovery::new(now, 123);
        let end = now + Duration::from_secs(30);
        for (attempt, base) in [1u64, 2, 4].into_iter().enumerate() {
            let previous = if attempt == 0 { now } else { recovery.next };
            if attempt != 0 {
                recovery.backoff(previous).unwrap();
            }
            let delay = recovery.next.duration_since(previous);
            assert!(delay >= Duration::from_millis(base * 800));
            assert!(delay <= Duration::from_millis(base * 1200));
            assert_eq!(recovery.deadline, end);
            assert!(recovery.begin(recovery.next).unwrap() <= end);
        }
        assert!(recovery.backoff(recovery.next).is_err());
        assert!(recovery.begin(end).is_err());
    }

    #[test]
    fn retained_queue_survives_read_error_but_late_reply_cannot_restore_cancelled_play() {
        use esp_shazam_core::{
            stream_buffer::OutputFormat,
            stream_wait::{Capacity, Slot},
        };
        let network = network_tests::ready_network();
        let lease = network.lease().unwrap();
        let (commands, receiver) = Commands::new(network.clone());
        commands.submit(Command::Play).unwrap();
        let generation = receiver.recv().unwrap().generation;
        let queue = Queue::new(network.clone());
        let peer = "192.0.2.20".parse().unwrap();
        let token = commands.token(generation).unwrap();
        let intent = queue
            .capacity
            .reserve(
                peer,
                network.clone(),
                lease,
                token,
                OutputFormat::Mp3,
                None,
                Instant::now(),
            )
            .unwrap();
        queue.snapshot.lock().unwrap().intent = Some(intent);
        let snapshot = QueueSnapshot::new(
            "same-item".into(),
            lease.address,
            None,
            OutputFormat::Mp3,
            lease.generation,
            intent,
        )
        .unwrap();
        queue.publish(snapshot.clone());
        let listener = Capacity::admit(
            &queue.capacity,
            intent,
            peer,
            lease.generation,
            OutputFormat::Mp3,
            None,
        )
        .unwrap();
        let debug = Slot::take(&queue.capacity, 2).unwrap();
        let status = recovery_status();
        let observation = ControlObservation {
            queue: &queue,
            commands: &commands,
            generation,
            lease,
            status: &status,
        };
        let mut recovery = None;
        assert_eq!(
            observation
                .finish(
                    &mut recovery,
                    Err(io::Error::from(io::ErrorKind::TimedOut)),
                    Instant::now(),
                    7
                )
                .unwrap(),
            State::Degraded
        );
        assert_eq!(status.lock().unwrap().state, State::Degraded);
        assert!(status.lock().unwrap().last_error.is_some());
        assert_eq!(queue.snapshot(), Some(snapshot.clone()));
        drop(listener);
        let retry = Capacity::admit(
            &queue.capacity,
            intent,
            peer,
            lease.generation,
            OutputFormat::Mp3,
            None,
        )
        .unwrap();
        assert_eq!(queue.capacity.held(), 2);
        commands.submit(Command::Stop).unwrap();
        assert!(observation_checkpoint(&queue, &commands, generation, &network, lease).is_err());
        assert!(observation
            .finish(
                &mut recovery,
                Ok(Some(esp_shazam_core::sonos_upnp::TransportState::Playing)),
                Instant::now(),
                7
            )
            .is_err());
        assert_eq!(status.lock().unwrap().state, State::Degraded);
        assert_eq!(queue.snapshot(), None);
        drop(retry);
        drop(debug);
        assert_eq!(queue.capacity.held(), 0);
    }

    #[test]
    fn startup_errors_and_invalid_reservations_cannot_enter_recovery() {
        with_observation(|observation, original| {
            observation.status.lock().unwrap().state = State::Connecting;
            let mut recovery = None;
            assert!(observation
                .finish(
                    &mut recovery,
                    Err(io::Error::from(io::ErrorKind::TimedOut)),
                    Instant::now(),
                    7
                )
                .is_err());
            assert!(recovery.is_none());
            assert!(!observation.queue.capacity.valid(original.sonos_intent));
        });
        with_observation(|observation, original| {
            observation.queue.capacity.clear(original.sonos_intent);
            let mut recovery = None;
            assert!(observation
                .finish(
                    &mut recovery,
                    Err(io::Error::from(io::ErrorKind::TimedOut)),
                    Instant::now(),
                    7
                )
                .is_err());
            assert!(recovery.is_none());
            assert_eq!(observation.queue.snapshot(), None);
        });
    }

    #[test]
    fn each_observation_uses_remaining_shared_deadline() {
        let now = Instant::now();
        let mut recovery = ControlRecovery::new(now, 11);
        assert_eq!(
            recovery.begin(now + Duration::from_secs(29)).unwrap(),
            now + Duration::from_secs(30)
        );
        assert_eq!(recovery.deadline, now + Duration::from_secs(30));
    }

    #[test]
    fn absent_intent_cannot_validate_an_observation() {
        let network = network_tests::ready_network();
        let lease = network.lease().unwrap();
        let (commands, _) = Commands::new(network.clone());
        let queue = Queue::new(network.clone());
        assert!(!queue.intent_valid());
        assert!(observation_checkpoint(&queue, &commands, 0, &network, lease).is_err());
    }

    #[test]
    fn foreign_source_revokes_delivery_and_late_matching_reply_cannot_restore_it() {
        use esp_shazam_core::sonos_upnp::TransportState;
        with_observation(|observation, original| {
            let mut recovery = None;
            assert!(observation
                .finish(&mut recovery, Ok(None), Instant::now(), 7)
                .is_err());
            assert_eq!(observation.queue.snapshot(), None);
            assert!(!observation.queue.capacity.valid(original.sonos_intent));
            assert!(observation
                .finish(
                    &mut recovery,
                    Ok(Some(TransportState::Playing)),
                    Instant::now(),
                    7
                )
                .is_err());
            assert_eq!(observation.queue.snapshot(), None);
            assert!(recovery.is_none());
        });
    }

    #[test]
    fn settings_changes_preserve_degraded_error_until_confirmed_playing() {
        let status = std::sync::Mutex::new(Snapshot {
            config: SonosConfig::default(),
            buffer_available: true,
            active_format: esp_shazam_core::stream_buffer::OutputFormat::Mp3,
            groups: vec![],
            active_player_id: Some("same-player".into()),
            state: State::Degraded,
            last_error: Some("control timeout".into()),
        });
        let config = SonosConfig {
            output_format: esp_shazam_core::stream_buffer::OutputFormat::Flac,
            ..Default::default()
        };
        save_config(&status, config.clone(), |_| Ok::<(), ()>(())).unwrap();
        assert_eq!(
            status.lock().unwrap().last_error.as_deref(),
            Some("control timeout")
        );
        assert_eq!(status.lock().unwrap().config, config);
        assert_eq!(
            status.lock().unwrap().active_format,
            esp_shazam_core::stream_buffer::OutputFormat::Mp3
        );
    }

    #[test]
    fn malformed_control_and_network_change_are_terminal() {
        assert!(!recoverable_observation_error(&io::Error::from(
            io::ErrorKind::InvalidData
        )));
        assert!(!recoverable_observation_error(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
        let network = network_tests::ready_network();
        let lease = network.lease().unwrap();
        let (commands, _) = Commands::new(network.clone());
        let queue = Queue::new(network.clone());
        network.invalidate();
        assert!(observation_checkpoint(&queue, &commands, 0, &network, lease).is_err());
    }
}
