// ABOUTME: One bounded Sonos controller worker owns discovery, TLS and our playback session.
// ABOUTME: The UI queues commands and reads snapshots without waiting for speaker I/O.

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
}

struct QueuedCommand {
    command: Command,
    generation: u64,
}

struct Commands {
    sender: std::sync::Mutex<(std::sync::mpsc::SyncSender<QueuedCommand>, u64)>,
}

impl Commands {
    fn new() -> (Self, std::sync::mpsc::Receiver<QueuedCommand>) {
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        (
            Self {
                sender: std::sync::Mutex::new((sender, 0)),
            },
            receiver,
        )
    }

    fn submit(&self, command: Command) -> Result<(), &'static str> {
        let mut sender = self.sender.lock().unwrap();
        let cancels = matches!(
            command,
            Command::Stop | Command::Select(_) | Command::SetAutoplay(false)
        );
        let generation = if cancels {
            sender
                .1
                .checked_add(1)
                .ok_or("Sonos command generation exhausted")?
        } else {
            sender.1
        };
        sender
            .0
            .try_send(QueuedCommand {
                command,
                generation,
            })
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => "Sonos command queue is full",
                std::sync::mpsc::TrySendError::Disconnected(_) => "Sonos worker has stopped",
            })?;
        sender.1 = generation;
        Ok(())
    }

    fn is_current(&self, generation: u64) -> bool {
        self.sender.lock().unwrap().1 == generation
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
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub config: SonosConfig,
    pub groups: Vec<DiscoveryGroup>,
    pub active_player_id: Option<String>,
    pub state: State,
    pub last_error: Option<String>,
}

const CHUNK_BYTES: usize = 1024;

struct Chunk {
    opcode: u8,
    fin: bool,
    offset: usize,
    total: usize,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct Incoming {
    message: protocol::MessageAssembler,
    frame: Option<(u8, bool, usize, usize)>,
}

impl Incoming {
    fn push(&mut self, chunk: Chunk) -> Result<Option<Vec<u8>>, protocol::Error> {
        let end = chunk
            .offset
            .checked_add(chunk.bytes.len())
            .ok_or(protocol::Error::TooLarge)?;
        if chunk.total > protocol::MAX_MESSAGE_BYTES || chunk.bytes.len() > CHUNK_BYTES {
            return Err(protocol::Error::TooLarge);
        }
        if end > chunk.total || (chunk.bytes.is_empty() && chunk.total != 0) {
            return Err(protocol::Error::InvalidFragment);
        }
        if let Some((opcode, fin, total, expected)) = self.frame {
            if (chunk.opcode, chunk.fin, chunk.total, chunk.offset)
                != (opcode, fin, total, expected)
            {
                return Err(protocol::Error::InvalidFragment);
            }
        } else if chunk.offset != 0 || !matches!(chunk.opcode, 0 | 1) {
            return Err(protocol::Error::InvalidFragment);
        }
        let kind = if chunk.offset == 0 && chunk.opcode == 1 {
            protocol::FragmentKind::Text
        } else {
            protocol::FragmentKind::Continuation
        };
        self.frame = (end < chunk.total).then_some((chunk.opcode, chunk.fin, chunk.total, end));
        self.message
            .push(kind, &chunk.bytes, chunk.fin && end == chunk.total)
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

// Error-only UART preview: fixed stack storage, no duplicate JSON tree or unbounded log line.
struct Diagnostic {
    bytes: [u8; 1024],
    len: usize,
    truncated: bool,
}

impl Diagnostic {
    fn new(args: std::fmt::Arguments<'_>) -> Self {
        let mut preview = Self {
            bytes: [0; 1024],
            len: 0,
            truncated: false,
        };
        let _ = std::fmt::write(&mut preview, args);
        preview
    }
}

impl std::fmt::Write for Diagnostic {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let mut take = text.len().min(self.bytes.len() - self.len);
        while !text.is_char_boundary(take) {
            take -= 1;
        }
        self.bytes[self.len..self.len + take].copy_from_slice(&text.as_bytes()[..take]);
        self.len += take;
        if take < text.len() {
            self.truncated = true;
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .write_str(std::str::from_utf8(&self.bytes[..self.len]).map_err(|_| std::fmt::Error)?)
    }
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

fn scope_matches(
    header: &protocol::MessageHeader,
    household: &str,
    scope: &protocol::Scope<'_>,
) -> bool {
    header.household_id.as_deref() == Some(household)
        && match scope {
            protocol::Scope::Player(id) => header.player_id.as_deref() == Some(id),
            protocol::Scope::Group(id) => header.group_id.as_deref() == Some(id),
            protocol::Scope::Session(id) => header.session_id.as_deref() == Some(id),
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

#[derive(Default)]
struct ReceiveDeadline(std::sync::atomic::AtomicU32);

impl ReceiveDeadline {
    fn remaining(&self, now_ms: u32) -> Option<u32> {
        use std::sync::atomic::Ordering;
        let stamp = match self.0.compare_exchange(
            0,
            now_ms.wrapping_add(1).max(1),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => now_ms.wrapping_add(1).max(1),
            Err(stamp) => stamp,
        };
        let elapsed = now_ms.wrapping_sub(stamp.wrapping_sub(1));
        (elapsed < 5000).then(|| 5000 - elapsed)
    }
    fn reset(&self) {
        self.0.store(0, std::sync::atomic::Ordering::Release);
    }
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

#[derive(Clone, Default)]
struct Queue(std::sync::Arc<std::sync::Mutex<Option<QueueSnapshot>>>);

impl Queue {
    fn snapshot(&self) -> Option<QueueSnapshot> {
        self.0.lock().unwrap().clone()
    }
    fn publish(&self, snapshot: QueueSnapshot) {
        *self.0.lock().unwrap() = Some(snapshot);
    }
    fn clear(&self) {
        *self.0.lock().unwrap() = None;
    }
}

fn release_session_after_error(
    session: &mut Option<String>,
    status: &mut Snapshot,
    queue: &Queue,
    policy: &mut protocol::Autoplay,
    now: u64,
    error_code: String,
) {
    policy.update(now, protocol::AutoplayEvent::OwnershipLost);
    *session = None;
    queue.clear();
    status.active_player_id = None;
    status.state = State::Error;
    status.last_error = Some(error_code);
}

fn queue_load_body(snapshot: &QueueSnapshot) -> serde_json::Value {
    serde_json::json!({ "queueBaseUrl": format!("http://{}{}", snapshot.host, sonos_queue::QUEUE_PATH),
        "queueVersion": snapshot.item_id, "itemId": snapshot.item_id, "playOnCompletion": true })
}

#[cfg(target_os = "espidf")]
mod device {
    use super::*;
    use crate::recognizer;
    use anyhow::{anyhow, bail, ensure, Context, Result};
    use esp_idf_svc::{
        hal::delay::TickType,
        nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault},
        sys,
        tls::X509,
        ws::client::{EspWebSocketClientConfig, EspWebSocketTransport},
    };
    use serde_json::{json, Value};
    use std::{
        ffi::{c_void, CString},
        net::{Ipv4Addr, SocketAddr, UdpSocket},
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Receiver, SyncSender},
            Arc, Mutex,
        },
        time::{Duration, Instant},
    };

    const TIMEOUT: Duration = Duration::from_secs(5);
    const ROOTS: &[u8] = concat!(include_str!("../assets/sonos-roots.pem"), "\0").as_bytes();

    #[derive(Clone)]
    pub struct Handle {
        commands: Arc<Commands>,
        status: Arc<Mutex<Snapshot>>,
        pending: Arc<Mutex<Option<Launch>>>,
        queue: Queue,
    }

    struct Launch {
        receiver: Receiver<QueuedCommand>,
        store: recognizer::SharedStore,
        nvs: EspNvs<NvsDefault>,
    }

    impl Handle {
        pub fn snapshot(&self) -> Snapshot {
            self.status.lock().unwrap().clone()
        }

        pub fn queue_snapshot(&self) -> Option<QueueSnapshot> {
            self.queue.snapshot()
        }

        fn ensure_started(&self) -> Result<()> {
            if let Some(launch) = self.pending.lock().unwrap().take() {
                let status = self.status.clone();
                let queue = self.queue.clone();
                let commands = self.commands.clone();
                crate::threads::ThreadSpec {
                    name: c"sonos",
                    core: esp_idf_svc::hal::cpu::Core::Core0,
                    priority: 2,
                    stack_bytes: 16 * 1024,
                }
                .spawn(move || {
                    let mut policy =
                        protocol::Autoplay::new(-50.0, -60.0).expect("Valid Sonos thresholds");
                    policy.update(
                        0,
                        protocol::AutoplayEvent::Enabled(status.lock().unwrap().config.autoplay),
                    );
                    Worker {
                        commands,
                        generation: 0,
                        status,
                        nvs: launch.nvs,
                        store: launch.store,
                        socket: None,
                        group: None,
                        session: None,
                        queue,
                        item_confirmed: false,
                        sequence: 0,
                        epoch: Instant::now(),
                        retry: None,
                        policy,
                        samples: LevelSamples::default(),
                        diagnostic_at: Instant::now(),
                    }
                    .run(launch.receiver);
                })?;
            }
            Ok(())
        }

        pub fn submit(&self, command: Command) -> Result<()> {
            if let Command::Select(id) = &command {
                ensure!(
                    !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control),
                    "Invalid player identity"
                );
            }
            self.ensure_started()?;
            self.commands
                .submit(command)
                .map_err(|error| anyhow!(error))
        }
    }

    pub fn start(store: recognizer::SharedStore, nvs: EspDefaultNvsPartition) -> Result<Handle> {
        let nvs = EspNvs::new(nvs, "sonos", true)?;
        let mut bytes = [0u8; 1024];
        let (config, last_error) = match nvs.get_str("config", &mut bytes) {
            Ok(saved) => decode_config(saved),
            Err(error) => (
                SonosConfig::default(),
                Some(format!("Cannot read saved Sonos configuration: {error}")),
            ),
        };
        let status = Arc::new(Mutex::new(Snapshot {
            last_error,
            config,
            groups: Vec::new(),
            active_player_id: None,
            state: State::Stopped,
        }));
        let (commands, receiver) = Commands::new();
        let enabled = status.lock().unwrap().config.autoplay;
        let handle = Handle {
            commands: Arc::new(commands),
            status,
            queue: Queue::default(),
            pending: Arc::new(Mutex::new(Some(Launch {
                receiver,
                nvs,
                store,
            }))),
        };
        // No task stack until an explicit command, unless autoplay was deliberately persisted.
        if enabled {
            handle.ensure_started()?;
        }
        Ok(handle)
    }

    fn protocol_error(error: protocol::Error) -> anyhow::Error {
        anyhow!("Sonos protocol error: {error:?}")
    }

    enum WireEvent {
        Connected,
        Gone,
        Data(Chunk),
    }

    struct Callback {
        sender: SyncSender<WireEvent>,
        failed: AtomicBool,
        control: Arc<TransportControl>,
    }

    impl Callback {
        fn fail(&self) {
            self.failed.store(true, Ordering::Release);
            self.control.cancelled.store(true, Ordering::Release);
        }
    }

    // IDF calls this on its WebSocket task. No parser, I/O, status lock or blocking send belongs here.
    unsafe extern "C" fn callback(
        arg: *mut c_void,
        _: sys::esp_event_base_t,
        id: i32,
        data: *mut c_void,
    ) {
        let context = &*(arg as *const Callback);
        let event = match id {
            sys::esp_websocket_event_id_t_WEBSOCKET_EVENT_CONNECTED => {
                context.control.receive_deadline.reset();
                WireEvent::Connected
            }
            sys::esp_websocket_event_id_t_WEBSOCKET_EVENT_ERROR
            | sys::esp_websocket_event_id_t_WEBSOCKET_EVENT_DISCONNECTED
            | sys::esp_websocket_event_id_t_WEBSOCKET_EVENT_CLOSED => WireEvent::Gone,
            sys::esp_websocket_event_id_t_WEBSOCKET_EVENT_DATA => {
                if data.is_null() {
                    context.fail();
                    return;
                }
                let data = &*(data as *const sys::esp_websocket_event_data_t);
                if data.payload_offset >= 0
                    && data.data_len >= 0
                    && data.payload_len >= 0
                    && (data.payload_offset as usize + data.data_len as usize)
                        >= data.payload_len as usize
                {
                    context.control.receive_deadline.reset();
                }
                if matches!(data.op_code, 9 | 10) {
                    return;
                }
                if data.op_code == 8 {
                    WireEvent::Gone
                } else {
                    if !matches!(data.op_code, 0 | 1)
                        || data.data_len < 0
                        || data.data_len as usize > CHUNK_BYTES
                        || data.payload_len < 0
                        || data.payload_len as usize > protocol::MAX_MESSAGE_BYTES
                        || data.payload_offset < 0
                        || (data.data_len != 0 && data.data_ptr.is_null())
                    {
                        context.fail();
                        return;
                    }
                    let bytes = if data.data_len == 0 {
                        Vec::new()
                    } else {
                        std::slice::from_raw_parts(data.data_ptr, data.data_len as usize).to_vec()
                    };
                    WireEvent::Data(Chunk {
                        opcode: data.op_code,
                        fin: data.fin,
                        offset: data.payload_offset as usize,
                        total: data.payload_len as usize,
                        bytes,
                    })
                }
            }
            _ => return,
        };
        if context.sender.try_send(event).is_err() {
            context.fail();
        }
    }

    // IDF 1.8's receive loop does not check stop between frame chunks. Keep its WS/TLS
    // implementations, but make each delegated TLS operation cancellable and nonblocking.
    #[derive(Default)]
    struct TransportControl {
        cancelled: AtomicBool,
        receive_deadline: ReceiveDeadline,
    }

    struct TransportGate {
        ssl: sys::esp_transport_handle_t,
        control: Arc<TransportControl>,
    }

    unsafe fn gate<'a>(handle: sys::esp_transport_handle_t) -> &'a TransportGate {
        &*sys::esp_transport_get_context_data(handle).cast::<TransportGate>()
    }

    fn tick_ms() -> u32 {
        (unsafe { sys::esp_timer_get_time() } / 1000) as u32
    }

    unsafe extern "C" fn gate_connect(
        handle: sys::esp_transport_handle_t,
        host: *const std::ffi::c_char,
        port: i32,
        _timeout: i32,
    ) -> i32 {
        let gate = gate(handle);
        let start = Instant::now();
        while !gate.control.cancelled.load(Ordering::Acquire) && start.elapsed() < TIMEOUT {
            // This path sets esp_tls_cfg.non_block=true; synchronous SSL connect does not.
            match sys::esp_transport_connect_async(gate.ssl, host, port, 5000) {
                1 => return 0,
                0 => std::thread::sleep(Duration::from_millis(5)),
                _ => return -1,
            }
        }
        -1
    }

    unsafe extern "C" fn gate_read(
        handle: sys::esp_transport_handle_t,
        bytes: *mut std::ffi::c_char,
        len: i32,
        _timeout: i32,
    ) -> i32 {
        let gate = gate(handle);
        while !gate.control.cancelled.load(Ordering::Acquire) {
            let Some(remaining) = gate.control.receive_deadline.remaining(tick_ms()) else {
                return -1;
            };
            let result = sys::esp_transport_read(gate.ssl, bytes, len, remaining.min(100) as i32);
            // Stock nonblocking SSL maps WANT_READ to timeout(0). Preserve split TLS records.
            if result != 0 && result != sys::ESP_TLS_ERR_SSL_WANT_WRITE {
                return result;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        -1
    }

    unsafe extern "C" fn gate_write(
        handle: sys::esp_transport_handle_t,
        bytes: *const std::ffi::c_char,
        len: i32,
        _timeout: i32,
    ) -> i32 {
        let gate = gate(handle);
        let start = Instant::now();
        let mut written = 0;
        while written < len
            && !gate.control.cancelled.load(Ordering::Acquire)
            && start.elapsed() < TIMEOUT
        {
            let result =
                sys::esp_transport_write(gate.ssl, bytes.add(written as usize), len - written, 100);
            if result > 0 {
                written += result;
            } else if matches!(
                result,
                0 | sys::ESP_TLS_ERR_SSL_WANT_READ | sys::ESP_TLS_ERR_SSL_WANT_WRITE
            ) {
                std::thread::sleep(Duration::from_millis(1));
            } else {
                return -1;
            }
        }
        if written == len {
            written
        } else {
            -1
        }
    }

    unsafe extern "C" fn gate_poll_read(handle: sys::esp_transport_handle_t, timeout: i32) -> i32 {
        let gate = gate(handle);
        if gate.control.cancelled.load(Ordering::Acquire) {
            return -1;
        }
        sys::esp_transport_poll_read(gate.ssl, timeout.clamp(0, 100))
    }

    unsafe extern "C" fn gate_poll_write(handle: sys::esp_transport_handle_t, timeout: i32) -> i32 {
        let gate = gate(handle);
        if gate.control.cancelled.load(Ordering::Acquire) {
            return -1;
        }
        sys::esp_transport_poll_write(gate.ssl, timeout.clamp(0, 100))
    }

    unsafe extern "C" fn gate_close(handle: sys::esp_transport_handle_t) -> i32 {
        sys::esp_transport_close(gate(handle).ssl)
    }

    struct Transports {
        websocket: sys::esp_transport_handle_t,
        gate: sys::esp_transport_handle_t,
        context: Box<TransportGate>,
    }

    impl Transports {
        fn new(
            config: &sys::esp_websocket_client_config_t,
            control: Arc<TransportControl>,
        ) -> Result<Self> {
            let ssl = unsafe { sys::esp_transport_ssl_init() };
            ensure!(!ssl.is_null(), "Cannot allocate Sonos TLS transport");
            let mut transports = Self {
                websocket: std::ptr::null_mut(),
                gate: std::ptr::null_mut(),
                context: Box::new(TransportGate { ssl, control }),
            };
            // The external-transport path skips IDF's transport setup; retain the wrapper's
            // explicit CA/subprotocol/headers here, with only the approved IP-name exception.
            unsafe {
                sys::esp_transport_ssl_set_cert_data(
                    ssl,
                    config.cert_pem,
                    std::ffi::CStr::from_ptr(config.cert_pem).to_bytes().len() as i32,
                );
                if config.skip_cert_common_name_check {
                    sys::esp_transport_ssl_skip_common_name_check(ssl);
                }
                transports.gate = sys::esp_transport_init();
                ensure!(
                    !transports.gate.is_null(),
                    "Cannot allocate Sonos transport gate"
                );
                sys::esp!(sys::esp_transport_set_context_data(
                    transports.gate,
                    (&mut *transports.context as *mut TransportGate).cast()
                ))?;
                sys::esp!(sys::esp_transport_set_func(
                    transports.gate,
                    Some(gate_connect),
                    Some(gate_read),
                    Some(gate_write),
                    Some(gate_close),
                    Some(gate_poll_read),
                    Some(gate_poll_write),
                    None
                ))?;
                transports.websocket = sys::esp_transport_ws_init(transports.gate);
                ensure!(
                    !transports.websocket.is_null(),
                    "Cannot allocate Sonos WebSocket transport"
                );
                sys::esp_transport_set_default_port(transports.websocket, 1443);
                // Match the component's internal setup: PONG must reach its watchdog and
                // every control-frame completion must reset our receive deadline.
                let websocket_config = sys::esp_transport_ws_config_t {
                    ws_path: c"/websocket/api".as_ptr(),
                    sub_protocol: config.subprotocol,
                    user_agent: config.user_agent,
                    headers: config.headers,
                    propagate_control_frames: true,
                    ..Default::default()
                };
                sys::esp!(sys::esp_transport_ws_set_config(
                    transports.websocket,
                    &websocket_config
                ))?;
            }
            Ok(transports)
        }
    }

    impl Drop for Transports {
        fn drop(&mut self) {
            // Caller destroys/joins the client first. Each external transport owns only itself.
            unsafe {
                if !self.websocket.is_null() {
                    sys::esp_transport_destroy(self.websocket);
                }
                if !self.gate.is_null() {
                    sys::esp_transport_destroy(self.gate);
                }
                sys::esp_transport_destroy(self.context.ssl);
            }
        }
    }

    struct Socket {
        raw: sys::esp_websocket_client_handle_t,
        callback: Option<Box<Callback>>,
        transports: Option<Transports>,
        receiver: Receiver<WireEvent>,
        incoming: Incoming,
        partial_since: Option<Instant>,
    }

    impl Socket {
        fn connect(group: &DiscoveryGroup) -> Result<Self> {
            Self::connect_uri(
                &format!("wss://{}:1443/websocket/api", group.host),
                ROOTS,
                true,
            )
        }

        fn connect_uri(uri: &str, roots: &'static [u8], skip_name_check: bool) -> Result<Self> {
            log_heap("before_connect");
            let (sender, receiver) = mpsc::sync_channel(12);
            let control = Arc::new(TransportControl::default());
            let mut callback_context = Box::new(Callback {
                sender,
                failed: AtomicBool::new(false),
                control: control.clone(),
            });
            let config = EspWebSocketClientConfig {
                disable_auto_reconnect: true,
                task_prio: 4,
                task_stack: 6144,
                buffer_size: CHUNK_BYTES,
                transport: EspWebSocketTransport::TransportOverSSL,
                subprotocol: Some("v1.api.smartspeaker.audio"),
                headers: Some("X-Sonos-Api-Key: 123e4567-e89b-12d3-a456-426655440000\r\n"),
                server_cert: Some(X509::pem_until_nul(roots)),
                // Approved exception for Sonos IP endpoints. The private CA chain remains required.
                skip_cert_common_name_check: skip_name_check,
                network_timeout_ms: TIMEOUT,
                pingpong_timeout_sec: Duration::from_secs(10),
                ping_interval_sec: Duration::from_secs(5),
                ..Default::default()
            };
            // svc 0.53's typed events discard FIN/offset and reject continuation opcodes.
            // Use its config conversion with the generated IDF event ABI instead.
            let (mut raw_config, _strings) =
                <(sys::esp_websocket_client_config_t, _)>::try_from(&config)?;
            let uri = CString::new(uri)?;
            raw_config.uri = uri.as_ptr();
            let transports = Transports::new(&raw_config, control)?;
            raw_config.ext_transport = transports.websocket;
            // IDF copies URI/headers during init; roots have static storage. The boxed callback
            // remains at a stable address until destroy has joined the client task.
            let raw = unsafe { sys::esp_websocket_client_init(&raw_config) };
            ensure!(!raw.is_null(), "Cannot allocate Sonos WebSocket client");
            let mut socket = Self {
                raw,
                callback: None,
                transports: Some(transports),
                receiver,
                incoming: Incoming::default(),
                partial_since: None,
            };
            let registered = unsafe {
                sys::esp_websocket_register_events(
                    raw,
                    sys::esp_websocket_event_id_t_WEBSOCKET_EVENT_ANY,
                    Some(callback),
                    (&mut *callback_context as *mut Callback).cast(),
                )
            };
            sys::esp!(registered)?;
            socket.callback = Some(callback_context);
            sys::esp!(unsafe { sys::esp_websocket_client_start(raw) })?;
            socket.check()?;
            match socket.receiver.recv_timeout(TIMEOUT)? {
                WireEvent::Connected => {
                    log_heap("connected");
                    Ok(socket)
                }
                WireEvent::Gone => bail!("Sonos connection failed"),
                WireEvent::Data(_) => bail!("Sonos data arrived before connection"),
            }
        }

        fn check(&self) -> Result<()> {
            ensure!(
                !self
                    .callback
                    .as_ref()
                    .unwrap()
                    .failed
                    .load(Ordering::Acquire),
                "Sonos receive queue overflow or invalid frame"
            );
            ensure!(
                !self
                    .partial_since
                    .is_some_and(|start| start.elapsed() >= TIMEOUT),
                "Sonos fragmented message timed out"
            );
            Ok(())
        }

        fn receive(&mut self, mut wait: Duration) -> Result<Option<protocol::Message>> {
            // Drain ready IDF chunks without a worker sleep between them. The queue is bounded.
            for _ in 0..12 {
                self.check()?;
                match self.receiver.recv_timeout(wait) {
                    Ok(WireEvent::Gone) => bail!("Sonos connection lost"),
                    Ok(WireEvent::Connected) => {}
                    Ok(WireEvent::Data(chunk)) => {
                        self.partial_since.get_or_insert_with(Instant::now);
                        if let Some(bytes) = self.incoming.push(chunk).map_err(protocol_error)? {
                            self.partial_since = None;
                            return Ok(Some(
                                protocol::parse_message(&bytes).map_err(|error| {
                                    let preview = Diagnostic::new(format_args!("{:?}", std::str::from_utf8(&bytes)));
                                    log::warn!("sonos: rejected_wire error={error:?} bytes={} truncated={} raw={preview}", bytes.len(), preview.truncated);
                                    protocol_error(error)
                                })?,
                            ));
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        self.check()?;
                        return Ok(None);
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => bail!("Sonos callback ended"),
                }
                wait = Duration::ZERO;
            }
            Ok(None)
        }

        fn send(&mut self, bytes: &[u8]) -> Result<()> {
            self.check()?;
            let ticks: TickType = TIMEOUT.into();
            let sent = unsafe {
                sys::esp_websocket_client_send_text(
                    self.raw,
                    bytes.as_ptr().cast(),
                    bytes.len() as _,
                    ticks.0,
                )
            };
            ensure!(
                sent >= 0 && sent as usize == bytes.len(),
                "Sonos send failed"
            );
            Ok(())
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            // Interrupt the nonblocking transport gate BEFORE IDF waits for its task to exit.
            self.transports
                .as_ref()
                .unwrap()
                .context
                .control
                .cancelled
                .store(true, Ordering::Release);
            let result = unsafe { sys::esp_websocket_client_destroy(self.raw) };
            if result != sys::ESP_OK {
                // Keep callback memory valid if IDF unexpectedly failed to stop its task.
                if let Some(callback) = self.callback.take() {
                    std::mem::forget(callback);
                }
                if let Some(transports) = self.transports.take() {
                    std::mem::forget(transports);
                }
                log::error!("Sonos client destroy failed: {result}");
            }
        }
    }

    /// Real TLS receive-only diagnostic, built solely for the explicit hardware fixture example.
    #[cfg(feature = "sonos-fixture")]
    #[allow(
        dead_code,
        reason = "Called by the example; Cargo also builds the normal library with this feature"
    )]
    pub fn check_transport(
        uri: &str,
        roots: &'static [u8],
        cancel_after: Option<Duration>,
    ) -> Result<()> {
        let mut socket = Socket::connect_uri(uri, roots, false)?;
        let connected = Instant::now();
        log::info!("sonos-fixture: connected");
        let mut baseline_seen = false;
        let mut cancelled = false;
        while connected.elapsed() < Duration::from_secs(15) {
            if cancel_after.is_some_and(|after| connected.elapsed() >= after) {
                log::info!(
                    "sonos-fixture: cancel_requested after_ms={}",
                    connected.elapsed().as_millis()
                );
                cancelled = true;
                break;
            }
            match socket.receive(Duration::from_millis(50)) {
                Ok(Some(_)) => {
                    baseline_seen = true;
                    log::info!("sonos-fixture: complete protocol message");
                }
                Ok(None) => {}
                Err(error) => {
                    log::info!(
                        "sonos-fixture: receive_end after_ms={} reason={error}",
                        connected.elapsed().as_millis()
                    );
                    break;
                }
            }
        }
        let before_drop = Instant::now();
        log::info!(
            "sonos-fixture: destroy_begin after_ms={}",
            connected.elapsed().as_millis()
        );
        drop(socket);
        log::info!(
            "sonos-fixture: destroyed after_ms={}",
            before_drop.elapsed().as_millis()
        );
        log_heap("fixture_destroyed");
        ensure!(
            baseline_seen,
            "Fixture never delivered its baseline message"
        );
        ensure!(
            cancel_after.is_none() || cancelled,
            "Fixture ended before explicit cancellation"
        );
        Ok(())
    }

    fn log_heap(phase: &str) {
        let caps = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
        let (free, largest) = unsafe {
            (
                sys::heap_caps_get_free_size(caps),
                sys::heap_caps_get_largest_free_block(caps),
            )
        };
        log::info!("sonos: phase={phase} internal_free={free} internal_largest={largest}");
    }

    fn board_ip() -> Result<Ipv4Addr> {
        // The default station netif is created by EspWifi. IDF owns it for the Wi-Fi lifetime.
        let netif = unsafe { sys::esp_netif_get_handle_from_ifkey(c"WIFI_STA_DEF".as_ptr()) };
        ensure!(!netif.is_null(), "Wi-Fi station interface is unavailable");
        let mut info = sys::esp_netif_ip_info_t::default();
        sys::esp!(unsafe { sys::esp_netif_get_ip_info(netif, &mut info) })?;
        let ip = Ipv4Addr::from(info.ip.addr.to_ne_bytes());
        ensure!(
            !ip.is_unspecified() && !ip.is_loopback(),
            "Wi-Fi has no LAN address"
        );
        Ok(ip)
    }

    fn discover(selected: Option<&str>) -> Result<Vec<DiscoveryGroup>> {
        let socket = UdpSocket::bind((board_ip()?, 0))?;
        socket.set_read_timeout(Some(Duration::from_millis(200)))?;
        socket.set_write_timeout(Some(Duration::from_millis(200)))?;
        socket.set_multicast_ttl_v4(2)?;
        socket.send_to(b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: urn:smartspeaker-audio:service:SpeakerGroup:1\r\n\r\n", (Ipv4Addr::new(239,255,255,250), 1900))?;
        let deadline = Instant::now() + Duration::from_secs(3);
        // The extra byte detects truncated oversized packets instead of accepting their prefix.
        let mut packet = [0u8; protocol::MAX_DISCOVERY_BYTES + 1];
        let mut groups: Vec<DiscoveryGroup> = Vec::new();
        while Instant::now() < deadline {
            match socket.recv_from(&mut packet) {
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
        Ok(groups)
    }

    struct Retry {
        player: String,
        at: Instant,
        attempt: usize,
    }

    struct Worker {
        commands: Arc<Commands>,
        generation: u64,
        status: Arc<Mutex<Snapshot>>,
        nvs: EspNvs<NvsDefault>,
        store: recognizer::SharedStore,
        socket: Option<Socket>,
        group: Option<DiscoveryGroup>,
        session: Option<String>,
        queue: Queue,
        item_confirmed: bool,
        policy: protocol::Autoplay,
        samples: LevelSamples,
        diagnostic_at: Instant,
        sequence: u64,
        epoch: Instant,
        retry: Option<Retry>,
    }

    impl Worker {
        fn now(&self) -> u64 {
            self.epoch.elapsed().as_millis() as u64
        }

        fn run(mut self, commands: Receiver<QueuedCommand>) {
            loop {
                match commands.recv_timeout(Duration::from_millis(50)) {
                    Ok(queued) => {
                        self.generation = queued.generation;
                        let command = queued.command;
                        let controls_session =
                            matches!(command, Command::Play | Command::Stop | Command::Select(_));
                        if let Err(error) = self.command(command) {
                            if controls_session {
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
                let sample = self.store.lock().unwrap().level_sample();
                let level = sample_level(sample, Instant::now());
                let action =
                    self.samples
                        .update(&mut self.policy, self.epoch, sample, Instant::now());
                let result = match action {
                    protocol::Action::Start => self.play(),
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
                for _ in 0..12 {
                    let Some(socket) = &mut self.socket else {
                        break;
                    };
                    match socket.receive(Duration::ZERO) {
                        Ok(Some(message)) => {
                            if let Err(e) = self.event(&message) {
                                self.fail(e);
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            self.fail(e);
                            break;
                        }
                    }
                }
                if self.retry.as_ref().is_some_and(|r| Instant::now() >= r.at) {
                    let mut retry = self.retry.take().unwrap();
                    match self
                        .resolve(&retry.player)
                        .and_then(|group| self.connect(group))
                    {
                        Ok(()) => {
                            // Reconnect observes status only. It never creates/reclaims a session.
                            self.status.lock().unwrap().state = State::Stopped;
                        }
                        Err(error) => {
                            self.socket = None;
                            self.group = None;
                            self.status.lock().unwrap().last_error =
                                Some(format!("Reconnect: {error}"));
                            retry.attempt += 1;
                            if retry.attempt < 3 {
                                retry.at = Instant::now() + Duration::from_secs(1 << retry.attempt);
                                self.retry = Some(retry);
                            }
                        }
                    }
                }
            }
        }

        fn fail(&mut self, error: anyhow::Error) {
            self.queue.clear();
            self.policy
                .update(self.now(), protocol::AutoplayEvent::OwnershipLost);
            let player = self
                .group
                .as_ref()
                .and_then(|_| self.status.lock().unwrap().config.player_id.clone());
            // A failed transport is not permission to take over again. Forget ownership locally.
            self.socket = None;
            self.session = None;
            self.group = None;
            let mut status = self.status.lock().unwrap();
            status.active_player_id = None;
            status.state = State::Error;
            status.last_error = Some(error.to_string());
            self.retry = player.map(|player| Retry {
                player,
                at: Instant::now() + Duration::from_secs(1),
                attempt: 0,
            });
        }

        fn save(&mut self, config: SonosConfig) -> Result<()> {
            self.nvs
                .set_str("config", &serde_json::to_string(&config)?)?;
            let mut status = self.status.lock().unwrap();
            status.last_error = None;
            status.config = config;
            Ok(())
        }

        fn command(&mut self, command: Command) -> Result<()> {
            match command {
                Command::Discover => {
                    let previous = self.status.lock().unwrap().state;
                    self.status.lock().unwrap().state = State::Discovering;
                    let selected = self.status.lock().unwrap().config.player_id.clone();
                    let result = discover(selected.as_deref());
                    self.status.lock().unwrap().state = previous;
                    let groups = result?;
                    let mut status = self.status.lock().unwrap();
                    status.groups = groups.into_iter().filter(|g| g.is_leader).collect();
                    status.last_error = None;
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
                Command::Play => self.play()?,
                Command::Stop => self.stop()?,
            }
            Ok(())
        }

        fn resolve(&mut self, player: &str) -> Result<DiscoveryGroup> {
            let groups = discover(Some(player))?;
            let group = resolve_group(&groups, player);
            self.status.lock().unwrap().groups =
                groups.into_iter().filter(|g| g.is_leader).collect();
            group.ok_or_else(|| anyhow!("Selected player's current group leader is unavailable"))
        }

        fn connect(&mut self, group: DiscoveryGroup) -> Result<()> {
            self.socket = Some(Socket::connect(&group)?);
            self.group = Some(group.clone());
            let info = self.request(
                "info:1",
                "getInfo",
                protocol::Scope::Player(&group.player_id),
                json!({}),
            )?;
            ensure!(
                info["playerId"].as_str() == Some(&group.player_id)
                    && info["householdId"].as_str() == Some(&group.household_id)
                    && info["groupId"].as_str() == Some(&group.group_id),
                "Sonos identity/topology changed since discovery"
            );
            self.request(
                "playback:1",
                "subscribe",
                protocol::Scope::Group(&group.group_id),
                json!({}),
            )?;
            Ok(())
        }

        fn play(&mut self) -> Result<()> {
            if !self.commands.is_current(self.generation) {
                return Ok(());
            }
            let result = self.play_current();
            if !self.commands.is_current(self.generation) {
                // A create reply must still be collected so cleanup targets only our session.
                self.stop()?;
                return Ok(());
            }
            result
        }

        fn play_current(&mut self) -> Result<()> {
            self.stop()?;
            let player = self
                .status
                .lock()
                .unwrap()
                .config
                .player_id
                .clone()
                .context("Choose a Sonos destination first")?;
            self.status.lock().unwrap().state = State::Connecting;
            let group = self.resolve(&player)?;
            ensure!(
                self.commands.is_current(self.generation),
                "Sonos Play cancelled"
            );
            self.connect(group.clone())?;
            let mut mac = [0u8; 6];
            sys::esp!(unsafe { sys::esp_efuse_mac_get_default(mac.as_mut_ptr()) })?;
            let context = format!(
                "esp-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            );
            let body = self.request(
                "playbackSession:1",
                "createSession",
                protocol::Scope::Group(&group.group_id),
                json!({"appId":"com.espanalogstreamer.source", "appContext":context}),
            )?;
            let session = body["sessionId"]
                .as_str()
                .filter(|s| protocol::validate_id(s).is_ok())
                .context("Sonos did not return a valid session")?
                .to_owned();
            self.session = Some(session.clone());
            self.policy
                .update(self.now(), protocol::AutoplayEvent::OwnershipAcquired);
            self.status.lock().unwrap().active_player_id = Some(player);
            let result = (|| -> Result<()> {
                self.request(
                    "playbackSession:1",
                    "subscribe",
                    protocol::Scope::Session(&session),
                    json!({}),
                )?;
                ensure!(
                    self.session.as_deref() == Some(&session),
                    "Sonos session lost before load"
                );
                let item = format!("{context}-{}-{}", self.now(), self.sequence);
                let queue = QueueSnapshot::new(item, board_ip()?)
                    .map_err(|error| anyhow!("Invalid Sonos queue: {error:?}"))?;
                let body = queue_load_body(&queue);
                // Sonos can request the queue before the command reply arrives.
                self.queue.publish(queue);
                self.item_confirmed = false;
                self.request(
                    "playbackSession:1",
                    "loadCloudQueue",
                    protocol::Scope::Session(&session),
                    body,
                )?;
                ensure!(
                    self.session.as_deref() == Some(&session),
                    "Sonos session lost during load"
                );
                let mut status = self.status.lock().unwrap();
                if status.state != State::Playing {
                    status.state = State::Buffering;
                }
                status.last_error = None;
                Ok(())
            })();
            if result.is_err() {
                let _ = self.stop();
            }
            result
        }

        fn stop(&mut self) -> Result<()> {
            self.queue.clear();
            self.policy
                .update(self.now(), protocol::AutoplayEvent::ManualStop);
            self.retry = None;
            // Only our session ID is eligible for cleanup, even if playback ownership changed.
            let result = match self.session.take() {
                Some(session) if self.socket.is_some() => self
                    .request(
                        "playbackSession:1",
                        "suspend",
                        protocol::Scope::Session(&session),
                        json!({}),
                    )
                    .map(|_| ()),
                _ => Ok(()),
            };
            self.socket = None;
            self.group = None;
            let mut status = self.status.lock().unwrap();
            status.active_player_id = None;
            status.state = State::Stopped;
            status.last_error = None;
            result
        }

        fn event(&mut self, message: &protocol::Message) -> Result<()> {
            let item = self.queue.snapshot().map(|queue| queue.item_id);
            let Some(group) = self.group.as_ref() else {
                return Ok(());
            };
            let scope = if message.header.namespace.starts_with("playbackSession") {
                let Some(session) = self.session.as_deref() else {
                    return Ok(());
                };
                protocol::Scope::Session(session)
            } else {
                protocol::Scope::Group(&group.group_id)
            };
            if !scope_matches(&message.header, &group.household_id, &scope) {
                return Ok(());
            }
            match message.event().map_err(|error| {
                let header = Diagnostic::new(format_args!("{:?}", message.header));
                log::warn!(
                    "sonos: rejected_event error={error:?} header_truncated={} header={header}",
                    header.truncated
                );
                let body = Diagnostic::new(format_args!("{}", message.body));
                log::warn!(
                    "sonos: rejected_event body_truncated={} body={body}",
                    body.truncated
                );
                protocol_error(error)
            })? {
                Some(protocol::Event::SessionError(error)) => {
                    // Household and owned session header scope were checked before event decoding.
                    let now = self.now();
                    release_session_after_error(
                        &mut self.session,
                        &mut self.status.lock().unwrap(),
                        &self.queue,
                        &mut self.policy,
                        now,
                        error.error_code,
                    );
                }
                Some(protocol::Event::SessionStatus(event))
                    if self.session.as_deref() == Some(&event.session_id) =>
                {
                    if !matches!(
                        event.session_state.as_str(),
                        "SESSION_STATE_CONNECTED" | "SESSION_STATE_ACTIVE"
                    ) {
                        self.policy
                            .update(self.now(), protocol::AutoplayEvent::OwnershipLost);
                        self.session = None;
                        self.queue.clear();
                        let mut status = self.status.lock().unwrap();
                        status.active_player_id = None;
                        status.state = State::Stopped;
                        status.last_error = Some(format!("Session ended: {}", event.session_state));
                    }
                }
                Some(protocol::Event::SessionInfo(info)) if info.suspended => {
                    self.policy
                        .update(self.now(), protocol::AutoplayEvent::OwnershipLost);
                    self.session = None;
                    self.queue.clear();
                    let mut status = self.status.lock().unwrap();
                    status.active_player_id = None;
                    status.state = State::Stopped;
                    status.last_error = Some("Our session was suspended".into());
                }
                Some(protocol::Event::PlaybackError(error))
                    if self.session.is_some()
                        && item.is_some()
                        && error.item_id.as_ref() == item.as_ref() =>
                {
                    bail!("Sonos playback error: {}", error.error_code);
                }
                Some(protocol::Event::PlaybackStatus(event))
                    if self.session.is_some() && item.is_some() =>
                {
                    if event.item_id.as_ref() != item.as_ref() {
                        // Subscription snapshots from before load can arrive during its reply.
                        if !self.item_confirmed {
                            return Ok(());
                        }
                        self.policy
                            .update(self.now(), protocol::AutoplayEvent::OwnershipLost);
                        self.session = None;
                        self.queue.clear();
                        let mut status = self.status.lock().unwrap();
                        status.active_player_id = None;
                        status.state = State::Stopped;
                        status.last_error = Some("Playback source changed".into());
                    } else {
                        self.item_confirmed = true;
                        self.status.lock().unwrap().state = match event.playback_state.as_str() {
                            "PLAYBACK_STATE_PLAYING" => State::Playing,
                            "PLAYBACK_STATE_BUFFERING" => State::Buffering,
                            _ => State::Stopped,
                        };
                    }
                }
                _ => {}
            }
            Ok(())
        }

        fn request(
            &mut self,
            namespace: &str,
            name: &str,
            scope: protocol::Scope<'_>,
            body: Value,
        ) -> Result<Value> {
            self.sequence = self
                .sequence
                .checked_add(1)
                .context("Sonos command counter exhausted")?;
            let id = self.sequence.to_string();
            let group = self.group.as_ref().context("Sonos is disconnected")?;
            let household = group.household_id.clone();
            let command = protocol::Command {
                namespace,
                command: name,
                household_id: &household,
                cmd_id: &id,
                scope,
            };
            let bytes = protocol::encode_command(&command, &body).map_err(protocol_error)?;
            let sent = self.now();
            let pending = protocol::PendingCommand {
                namespace,
                command: name,
                cmd_id: &id,
                sent_at_ms: sent,
                deadline_ms: sent + TIMEOUT.as_millis() as u64,
            };
            if matches!(name, "createSession" | "loadCloudQueue") {
                ensure!(
                    self.commands.is_current(self.generation),
                    "Sonos Play cancelled"
                );
            }
            self.socket
                .as_mut()
                .context("Sonos is disconnected")?
                .send(&bytes)?;
            while self.now() < pending.deadline_ms {
                let message = self
                    .socket
                    .as_mut()
                    .context("Sonos is disconnected")?
                    .receive(Duration::from_millis(50))?;
                if let Some(message) = message {
                    if message.header.cmd_id.as_deref() == Some(&id)
                        && !scope_matches(&message.header, &household, &command.scope)
                    {
                        continue;
                    }
                    match message.match_reply(&pending, self.now()) {
                        protocol::ReplyMatch::Success => {
                            ensure!(
                                message.body.get("errorCode").is_none(),
                                "Sonos rejected {name}: {}",
                                message.body["errorCode"]
                            );
                            return Ok(message.body);
                        }
                        protocol::ReplyMatch::Failure => bail!(
                            "Sonos rejected {name}: {}",
                            message.body.get("errorCode").unwrap_or(&Value::Null)
                        ),
                        protocol::ReplyMatch::Expired => break,
                        protocol::ReplyMatch::Unrelated => self.event(&message)?,
                    }
                }
            }
            bail!("Sonos {name} timed out")
        }
    }
}
#[cfg(target_os = "espidf")]
pub use device::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_header_scoped_eviction_releases_ownership_and_requires_quiet_rearm() {
        let message = protocol::parse_message(br#"[{"namespace":"playbackSession","householdId":"house","sessionId":"owned","name":"sessionError","type":"sessionError"},{"_objectType":"sessionError","errorCode":"ERROR_SESSION_EVICTED","reason":"Replaced"}]"#).unwrap();
        assert!(scope_matches(
            &message.header,
            "house",
            &protocol::Scope::Session("owned")
        ));
        assert!(!scope_matches(
            &message.header,
            "house",
            &protocol::Scope::Session("foreign")
        ));
        let Some(protocol::Event::SessionError(error)) = message.event().unwrap() else {
            panic!("captured eviction must decode");
        };
        let mut session = Some("owned".to_owned());
        let mut status = Snapshot {
            config: SonosConfig {
                player_id: Some("selected".into()),
                autoplay: true,
            },
            groups: vec![],
            active_player_id: Some("selected".into()),
            state: State::Playing,
            last_error: None,
        };
        let queue = Queue::default();
        queue.publish(QueueSnapshot::new("item".into(), "192.0.2.10".parse().unwrap()).unwrap());
        let mut policy = protocol::Autoplay::new(-50.0, -60.0).unwrap();
        policy.update(0, protocol::AutoplayEvent::Enabled(true));
        policy.update(0, protocol::AutoplayEvent::OwnershipAcquired);
        release_session_after_error(
            &mut session,
            &mut status,
            &queue,
            &mut policy,
            100,
            error.error_code,
        );
        assert!(session.is_none());
        assert!(queue.snapshot().is_none());
        assert!(status.active_player_id.is_none());
        assert_eq!(status.config.player_id.as_deref(), Some("selected"));
        assert_eq!(status.last_error.as_deref(), Some("ERROR_SESSION_EVICTED"));
        for ms in (200..=12000).step_by(100) {
            assert_eq!(
                policy.update(ms, protocol::AutoplayEvent::Level(-43.0)),
                protocol::Action::None
            );
        }
        for ms in (13000..=23000).step_by(100) {
            assert_eq!(
                policy.update(ms, protocol::AutoplayEvent::Level(-85.0)),
                protocol::Action::None
            );
        }
        assert_eq!(
            policy.update(23100, protocol::AutoplayEvent::Level(-43.0)),
            protocol::Action::None
        );
        assert_eq!(
            policy.update(23600, protocol::AutoplayEvent::Level(-43.0)),
            protocol::Action::Start
        );
    }

    #[test]
    fn rejected_message_diagnostic_is_bounded_utf8_and_reports_truncation() {
        let long = "é".repeat(600);
        let preview = Diagnostic::new(format_args!("prefix:{long}"));
        assert!(preview.truncated);
        assert!(preview.len <= 1024);
        assert!(preview.to_string().starts_with("prefix:"));
        let preview = Diagnostic::new(format_args!("raw={:?}", "line\nnext"));
        assert!(!preview.truncated);
        assert_eq!(preview.to_string(), "raw=\"line\\nnext\"");
    }

    #[test]
    fn accepted_stop_disable_and_selection_cancel_earlier_play_but_not_later_play() {
        for cancel in [
            Command::Stop,
            Command::SetAutoplay(false),
            Command::Select("other".into()),
        ] {
            let (commands, receiver) = Commands::new();
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
        let (commands, receiver) = Commands::new();
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
    fn events_and_replies_require_exact_household_and_scope() {
        let message = protocol::parse_message(br#"[{"namespace":"playback:1","householdId":"house","groupId":"group","type":"playbackStatus"},{"playbackState":"PLAYBACK_STATE_PLAYING"}]"#).unwrap();
        assert!(scope_matches(
            &message.header,
            "house",
            &protocol::Scope::Group("group")
        ));
        assert!(!scope_matches(
            &message.header,
            "other",
            &protocol::Scope::Group("group")
        ));
        assert!(!scope_matches(
            &message.header,
            "house",
            &protocol::Scope::Group("other")
        ));
        assert!(!scope_matches(
            &message.header,
            "house",
            &protocol::Scope::Session("group")
        ));
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
    fn trickled_reads_share_one_deadline_including_clock_wrap() {
        for start in [100, u32::MAX - 100] {
            let deadline = ReceiveDeadline::default();
            assert_eq!(deadline.remaining(start), Some(5000));
            for elapsed in [100, 4999] {
                assert_eq!(
                    deadline.remaining(start.wrapping_add(elapsed)),
                    Some(5000 - elapsed)
                );
            }
            assert_eq!(deadline.remaining(start.wrapping_add(5000)), None);
            deadline.reset();
            assert_eq!(deadline.remaining(start.wrapping_add(6000)), Some(5000));
        }
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
        let (config, error) = decode_config(Some(r#"{"player_id":"player","autoplay":true}"#));
        assert_eq!(config.player_id.as_deref(), Some("player"));
        assert!(config.autoplay);
        assert_eq!(error, None);
        assert_eq!(decode_config(None), (SonosConfig::default(), None));
    }

    #[test]
    fn queue_publication_and_cleanup_share_one_snapshot_across_handles() {
        let queue = Queue::default();
        let reader = queue.clone();
        assert_eq!(reader.snapshot(), None);
        let snapshot = QueueSnapshot {
            item_id: "item-first".into(),
            host: "192.0.2.10".parse().unwrap(),
        };
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

    #[test]
    fn load_queue_points_to_board_and_uses_the_published_item_as_version() {
        let snapshot = QueueSnapshot {
            item_id: "item-live".into(),
            host: "192.0.2.10".parse().unwrap(),
        };
        assert_eq!(
            queue_load_body(&snapshot),
            serde_json::json!({
                "queueBaseUrl":"http://192.0.2.10/api/sonos/queue/v2.1",
                "queueVersion":"item-live", "itemId":"item-live", "playOnCompletion":true
            })
        );
    }

    fn chunk(opcode: u8, fin: bool, offset: usize, total: usize, bytes: &[u8]) -> Chunk {
        Chunk {
            opcode,
            fin,
            offset,
            total,
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn idf_chunks_and_websocket_continuations_form_one_message() {
        let mut input = Incoming::default();
        assert_eq!(input.push(chunk(1, false, 0, 2, &[0xc3])), Ok(None));
        assert_eq!(input.push(chunk(1, false, 1, 2, &[0xa9])), Ok(None));
        assert_eq!(
            input.push(chunk(0, true, 0, 1, b"!")),
            Ok(Some("é!".as_bytes().to_vec()))
        );
    }

    #[test]
    fn final_frame_is_complete_only_after_all_idf_chunks() {
        let mut input = Incoming::default();
        assert_eq!(input.push(chunk(1, true, 0, 3, b"ab")), Ok(None));
        assert_eq!(
            input.push(chunk(1, true, 2, 3, b"c")),
            Ok(Some(b"abc".to_vec()))
        );
    }

    #[test]
    fn broken_chunk_offsets_or_metadata_fail_closed() {
        for next in [
            chunk(1, true, 2, 3, b"c"),
            chunk(1, false, 1, 3, b"bc"),
            chunk(0, true, 1, 3, b"bc"),
        ] {
            let mut input = Incoming::default();
            assert_eq!(input.push(chunk(1, true, 0, 3, b"a")), Ok(None));
            assert!(input.push(next).is_err());
        }
    }

    #[test]
    fn frame_and_callback_chunk_limits_are_independent() {
        let mut input = Incoming::default();
        assert!(input.push(chunk(1, true, 0, 8193, b"a")).is_err());
        assert!(input
            .push(chunk(1, true, 0, 1025, &vec![b'a'; 1025]))
            .is_err());
        let mut input = Incoming::default();
        for offset in (0..8192).step_by(1024) {
            let result = input
                .push(chunk(1, true, offset, 8192, &vec![b'a'; 1024]))
                .unwrap();
            if offset == 7168 {
                assert_eq!(result.unwrap().len(), 8192);
            } else {
                assert_eq!(result, None);
            }
        }
    }
}
