// ABOUTME: The LINE IN stream on port 8000: serves the selected compressed codec and diagnostic 24-bit WAV at 48 kHz stereo.
// ABOUTME: One core 1 encoder shares compressed frames; listeners copy out before socket work and drop if they fall behind.

use esp_shazam_core::network_lifecycle::{Lease as NetworkLease, Network};
use std::io;
use std::net::{SocketAddr, TcpStream};
#[cfg(feature = "admission-resources")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::encoder_telemetry::{self, EncoderMetrics, RawReadMetrics, Reason, Sample};
use crate::stream_codec as codec;
use crate::stream_pipeline::{
    CompressedListener, ConnectionResources, EncoderObservation, EncoderPump, EncoderStep, End,
    ListenerStep, CAUGHT_UP_SLEEP, CHUNK_FRAMES, FRESH_AUDIO_TIMEOUT,
};
use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::mdns::EspMdns;
use esp_idf_svc::sys;
use esp_shazam_core::stream_buffer::{BufferPreset, OutputFormat};
use esp_shazam_core::stream_http::{
    self, DeadlineWriter, Format, ListenerPolicy, Route, Status, WriterDiagnostics,
};
use esp_shazam_core::stream_ring::{Cursor, ReadError};
use esp_shazam_core::stream_wait::{
    cursor_before, read_atomic_fresh_with, Capacity, Slot, WaitError,
};
use esp_shazam_core::wav;

use crate::audio_in::{SharedStreamRing, CAPTURE_RATE_HZ};
use crate::config::CONFIG;
use crate::listener_telemetry::{
    self, Departure, EndCode, ErrorCode, Payload, Record, RefusalReason, EVENTS,
};
use crate::psram::PsramBytes;
use crate::socket_close;
use crate::threads::ThreadSpec;

use crate::stream_runtime::{Epoch, Runtime, Worker};
static LISTENERS: OnceLock<Arc<Capacity>> = OnceLock::new();
static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();

pub(crate) fn try_active_format() -> Option<OutputFormat> {
    RUNTIME.get()?.try_active_format()
}

pub fn active_format() -> OutputFormat {
    RUNTIME
        .get()
        .map_or(OutputFormat::Flac, |runtime| runtime.active_format())
}

pub fn select_format(
    format: OutputFormat,
    force: bool,
    token: Arc<Mutex<bool>>,
) -> io::Result<bool> {
    let runtime = RUNTIME
        .get()
        .ok_or_else(|| io::Error::other("stream encoder unavailable"))?;
    if force {
        runtime.play_format(format, token)
    } else {
        runtime.switch(format, false, token, Duration::from_secs(3))
    }
}
pub(crate) fn capacity() -> Arc<Capacity> {
    LISTENERS
        .get_or_init(|| Arc::new(Capacity::default()))
        .clone()
}
static RAW_READ_MAX_US: AtomicU32 = AtomicU32::new(0);

/// Interval maximum of one atomic raw read attempt, including any preemption during the copy.
pub fn take_raw_read_max_us() -> u32 {
    RAW_READ_MAX_US.swap(0, Ordering::Relaxed)
}

/// A compressed listener requires byte-addressable PSRAM for its complete encoded frame.
/// Small headers and the WAV's small buffers retain the normal allocator's preferred placement.
enum ListenerBuffer {
    Wav,
    Compressed(PsramBytes),
}

impl ListenerBuffer {
    fn try_new(format: Format) -> io::Result<Self> {
        match format {
            Format::Wav => Ok(Self::Wav),
            Format::Flac | Format::Mp3 => PsramBytes::try_new(match format {
                Format::Flac => OutputFormat::Flac.max_frame_bytes(),
                Format::Mp3 => OutputFormat::Mp3.max_frame_bytes(),
                Format::Wav => unreachable!(),
            })
            .map(Self::Compressed),
        }
    }
}

struct ListenerJob {
    connection: ListenerConnection,
    ring: SharedStreamRing,
    buffer: ListenerBuffer,
    preset: Option<BufferPreset>,
    policy: ListenerPolicy,
    network: Network,
    network_lease: NetworkLease,
}

type OutputMailbox = crate::stream_workers::Mailbox<ListenerJob>;

fn output_job(job: ListenerJob) {
    let ListenerJob {
        connection,
        ring,
        buffer,
        preset,
        policy,
        network,
        network_lease,
    } = job;
    let resources = connection
        .resources
        .as_ref()
        .expect("listener owns resources");
    EVENTS.publish(Record {
        event_us: event_us(),
        peer: connection.peer,
        payload: Payload::Joined {
            format: match &buffer {
                ListenerBuffer::Wav => Format::Wav,
                ListenerBuffer::Compressed(_) => resources
                    .lease
                    .as_ref()
                    .expect("compressed lease")
                    .epoch
                    .format
                    .format(),
            },
            held: resources.slot.taken(),
            owner_id: resources.slot.owner_id(),
            sonos_intent: resources.slot.intent_id(),
        },
    });
    listen(
        connection,
        ring,
        buffer,
        preset,
        policy,
        network,
        network_lease,
    );
}

/// Keeps the listener slot until its socket close succeeds, including dispatch failure.
struct ListenerConnection {
    resources: Option<ConnectionResources<TcpStream>>,
    peer: Option<SocketAddr>,
}

impl ListenerConnection {
    fn socket(&self) -> &TcpStream {
        &self
            .resources
            .as_ref()
            .expect("listener owns resources")
            .socket
    }
}

impl Drop for ListenerConnection {
    fn drop(&mut self) {
        if let Some(resources) = self.resources.take() {
            match listener_telemetry::close_and_report(
                resources,
                &EVENTS,
                self.peer,
                event_us,
                |socket| socket_close::close(socket).map(|report| report.elapsed),
            ) {
                Ok(()) => {}
                Err(_failure) => {
                    // Keep the failure's owned socket and this slot alive through restart.
                    // Logging here could block recovery on a lock held by another task.
                    // SAFETY: ESP-IDF is initialized, and restart never returns or unwinds.
                    // The failed owned socket and slot stay held until reset reclaims them.
                    unsafe { esp_idf_svc::sys::esp_restart() }
                }
            }
        }
    }
}

/// The stream's port. The page and `/api/state` stay on port 80.
pub const PORT: u16 = 8000;
/// WAV and compressed listeners share the slots, and a third listener of either gets 503. Each one takes a
/// permanent worker, TCP buffers that prefer PSRAM, and Wi-Fi: 2.3 Mbit/s for the WAV, less for compressed audio.
const MAX_LISTENERS: usize = 2;
// Both compressed codecs encode the capture sample rate.
const _: () = assert!(codec::SAMPLE_RATE_HZ == CAPTURE_RATE_HZ);
/// A client gets this long to send its whole request head, which may be at most `MAX_HEAD_BYTES`,
/// and a separate allowance to take a refusal. Each admission worker handles one head at a time.
const HEAD_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_HEAD_BYTES: u64 = 4096;
const ADMISSION_WORKERS: &[&std::ffi::CStr] = &[c"stream-in-1", c"stream-in-2"];
const _: () = assert!(!ADMISSION_WORKERS.is_empty() && ADMISSION_WORKERS.len() <= 2);
#[cfg(feature = "admission-resources")]
static ADMISSION_READY: AtomicBool = AtomicBool::new(false);

/// The server threads share core 1 with the encoder. Their priority, 7, is
/// above the recognizer's (5), so a fingerprint can't starve a listener, and below the audio thread
/// (10), lwIP (18) and Wi-Fi (23).
const ACCEPT_THREAD: ThreadSpec = ThreadSpec {
    name: ADMISSION_WORKERS[0],
    core: Core::Core1,
    priority: 7,
    stack_bytes: 4 * 1024,
};
const ENCODER_THREAD: ThreadSpec = ThreadSpec {
    name: c"stream-encode",
    stack_bytes: 8 * 1024,
    ..ACCEPT_THREAD
};
const LISTENER_THREAD: ThreadSpec = ThreadSpec {
    name: c"stream-out",
    ..ACCEPT_THREAD
};

/// Listens on port 8000 with two admission workers, and advertises the stream over mDNS as
/// "esp-shazam stream", next to the page's "esp-shazam".
pub fn start(ring: SharedStreamRing, mdns: &mut EspMdns, network: Network) -> Result<()> {
    let listener = Arc::new(Mutex::new(
        crate::stream_admission::GenerationListener::new(SocketAddr::from(([0, 0, 0, 0], PORT))),
    ));
    let (runtime, worker) = Runtime::new(OutputFormat::Flac, CONFIG.shazam_enabled)?;
    RUNTIME
        .set(runtime.clone())
        .map_err(|_| anyhow::anyhow!("stream already started"))?;
    let listeners = capacity();
    // Initialize native mutex storage before admission/listener tasks start.
    let _ = listeners.held();
    let layout = esp_shazam_core::stream_wait::capacity_metadata_layout();
    log::info!("stream-capacity-layout: slot={} owner={} intent={} registry={} mutex_registry={} arc_data_offset={} arc_allocation={} native_mutex_backing=excluded allocator_overhead=excluded", layout[0], layout[1], layout[2], layout[3], layout[4], layout[5], layout[6]);
    let encoder_listeners = listeners.clone();
    let (encoder_raw, encoder_runtime) = (ring.clone(), runtime.clone());
    ENCODER_THREAD
        .spawn(move || encode(encoder_raw, encoder_runtime, encoder_listeners, worker))?;
    let gate = Arc::new(crate::stream_admission::StartupGate::new());
    // Startup-only receipt: retain these stacks through native Sonos/TLS setup.
    // SAFETY: heap capability queries are read-only after ESP-IDF initialization.
    let internal_before =
        unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT) };
    let outputs = Arc::new(OutputMailbox::new());
    log::info!("stream-output-layout: job={} mailbox={} native_backing=excluded allocator_overhead=excluded", std::mem::size_of::<ListenerJob>(), std::mem::size_of::<OutputMailbox>());
    for index in 0..2 {
        let mailbox = outputs.clone();
        if let Err(error) = LISTENER_THREAD.spawn(move || mailbox.work(index, output_job)) {
            gate.exit();
            outputs.close();
            return Err(error);
        }
    }
    if !outputs.wait_ready(Instant::now() + Duration::from_secs(2)) {
        gate.exit();
        outputs.close();
        return Err(anyhow::anyhow!("output workers did not enter initial wait"));
    }
    for &name in ADMISSION_WORKERS {
        let (listener, ring, runtime, listeners, worker_gate) = (
            listener.clone(),
            ring.clone(),
            runtime.clone(),
            listeners.clone(),
            gate.clone(),
        );
        let network = network.clone();
        let worker_outputs = outputs.clone();
        if let Err(error) = (ThreadSpec {
            name,
            ..ACCEPT_THREAD
        })
        .spawn(move || {
            if worker_gate.wait() {
                accept(listener, ring, runtime, listeners, network, worker_outputs);
            }
        }) {
            gate.exit();
            outputs.close();
            return Err(error);
        }
    }
    // SAFETY: same startup-only read-only allocator query as above.
    let internal_after =
        unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT) };
    log::info!("stream-output-startup: workers=2 stack_bytes_each=4096 internal_free_before={} internal_free_after={} scope=output_mailbox_and_output_and_admission_startup", internal_before, internal_after);
    outputs.run();
    gate.run();
    #[cfg(feature = "admission-resources")]
    ADMISSION_READY.store(true, Ordering::Release);
    // The stream works without the advertisement, so a failure is only logged.
    if let Err(e) = mdns.add_service(
        Some("esp-shazam stream"),
        "_http",
        "_tcp",
        PORT,
        &[("path", Format::Wav.path())],
    ) {
        log::warn!("stream: mDNS service not added: {e}");
    }
    log::info!("stream: admission workers started on port {PORT}");
    Ok(())
}

fn accept(
    listener: Arc<Mutex<crate::stream_admission::GenerationListener>>,
    ring: SharedStreamRing,
    runtime: Arc<Runtime>,
    listeners: Arc<Capacity>,
    network: Network,
    outputs: Arc<OutputMailbox>,
) -> ! {
    loop {
        let accepted = listener
            .lock()
            .expect("stream listener mutex poisoned")
            .accept(&network);
        match accepted {
            Ok(Some((sock, _, lease))) => {
                answer(sock, &ring, &runtime, &listeners, &network, lease, &outputs)
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                log::warn!("stream: accept failed: {e}");
                // lwIP drops a connection it has no socket for. Pause in case an error repeats at once.
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

// Only the permanent metrics owner calls this, after capture formatting returns.
#[cfg(feature = "admission-resources")]
#[inline(never)]
pub(crate) fn export_admission_resources(sequence: &mut u64) {
    *sequence += 1;
    let device_us = event_us();
    let ready = ADMISSION_READY.load(Ordering::Acquire);
    let mut found = [false; 2];
    let mut hwm = [0; 2];
    let mut task_count = 0;
    let mut query_us = 0;
    let mut metrics_hwm = 0;
    if ready {
        let started = event_us();
        for (index, name) in ADMISSION_WORKERS.iter().enumerate() {
            // SAFETY: Release readiness follows every successful spawn and terminal Run.
            // Run enters accept -> !; no deletion, replacement or panic recovery exists.
            // panic_abort ends the boot on panic. Never extend this to transient tasks.
            let handle = unsafe { sys::xTaskGetHandle(name.as_ptr()) };
            if !handle.is_null() {
                found[index] = true;
                // SAFETY: the permanent worker lifetime above outlives this local lookup.
                hwm[index] = unsafe { sys::uxTaskGetStackHighWaterMark(handle) };
            }
        }
        // SAFETY: count has no handle; NULL watermark intentionally selects this live owner.
        task_count = unsafe { sys::uxTaskGetNumberOfTasks() };
        metrics_hwm = unsafe { sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
        query_us = event_us() - started;
    }
    // IDF reports historical untouched fill bytes, not worst-case stack safety.
    // Task count is sampled, not peak, and includes deleted tasks awaiting idle cleanup.
    log::info!("admission-resources: schema=1 seq={} device_us={} expected_workers={} ready={} worker1_found={} worker1_hwm_bytes={} worker2_found={} worker2_hwm_bytes={} task_count={} query_us={} metrics_hwm_bytes={}",
        sequence, device_us, ADMISSION_WORKERS.len(), ready, found[0], hwm[0], found[1], hwm[1], task_count, query_us, metrics_hwm);
}

/// Reads one request and answers it: an owned job for a stream, or a refusal.
fn answer(
    sock: TcpStream,
    ring: &SharedStreamRing,
    runtime: &Arc<Runtime>,
    listeners: &Arc<Capacity>,
    network: &Network,
    network_lease: NetworkLease,
    outputs: &OutputMailbox,
) {
    if !network.is_current(network_lease) {
        return;
    }
    let peer_address = sock.peer_addr().ok();
    let peer = peer_address.map_or_else(|| "unknown peer".to_owned(), |addr| addr.to_string());
    // One deadline for the whole head: a per-read timeout would let a client that sends a byte
    // every 1.9 s hold the accept thread for hours.
    let head =
        crate::stream_admission::read_head(&sock, Instant::now() + HEAD_TIMEOUT, MAX_HEAD_BYTES);
    let request_line = match head {
        Ok(line) => line,
        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
            refuse(sock, &peer, "(unreadable)", Status::BadRequest);
            return;
        }
        Err(e) => {
            // Timed out, reset or closed before the head ended: nobody to answer.
            log::info!("stream: {peer}: no request ({e})");
            return;
        }
    };
    if !network.is_current(network_lease) {
        return;
    }
    match stream_http::request_network_generation(&request_line) {
        Ok(Some(generation)) if generation != network_lease.generation => {
            refuse(sock, &peer, &request_line, Status::ServiceUnavailable);
            return;
        }
        Err(_) => {
            refuse(sock, &peer, &request_line, Status::BadRequest);
            return;
        }
        _ => {}
    }
    let sonos_intent = match stream_http::request_sonos_intent(&request_line) {
        Ok(intent) => intent,
        Err(_) => {
            refuse(sock, &peer, &request_line, Status::BadRequest);
            return;
        }
    };
    // `?exact=1` keeps lag terminal for archive readers; everyone else skips forward and stays.
    let policy = match stream_http::request_listener_policy(&request_line) {
        Ok(policy) => policy,
        Err(_) => {
            refuse(sock, &peer, &request_line, Status::BadRequest);
            return;
        }
    };
    let (route, lease) = runtime.admit(&request_line);
    match route {
        Route::Stream(_) | Route::Buffered(_) => {
            let (format, preset) = match route {
                Route::Stream(format) => (format, None),
                Route::Buffered(preset) => (
                    lease
                        .as_ref()
                        .expect("compressed lease")
                        .epoch
                        .format
                        .format(),
                    Some(preset),
                ),
                Route::Refuse(_) => unreachable!(),
            };
            // Reject unsupported close configuration before taking a slot or allocating audio.
            if let Err(error) = socket_close::configure(&sock) {
                log::warn!("stream: {peer}: close policy unavailable ({error})");
                refuse(sock, &peer, &request_line, Status::ServiceUnavailable);
                return;
            }
            // Reconnect waits at most 12 seconds for the prior owner's checked close.
            // The two admission workers may each spend this finite budget on replacement.
            let deadline = Instant::now() + Duration::from_secs(12);
            let slot = if let Some(intent) = sonos_intent {
                loop {
                    let candidate = match peer_address {
                        Some(peer) => match Capacity::admit_request(
                            listeners,
                            &request_line,
                            peer.ip(),
                            network_lease.generation,
                        ) {
                            Ok(candidate) => candidate,
                            Err(status) => {
                                refuse(sock, &peer.to_string(), &request_line, status);
                                return;
                            }
                        },
                        None => {
                            refuse(sock, &peer, &request_line, Status::ServiceUnavailable);
                            return;
                        }
                    };
                    if candidate.is_some()
                        || !listeners.valid(intent)
                        || Instant::now() >= deadline
                        || !network.is_current(network_lease)
                    {
                        break candidate;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            } else {
                Slot::take(listeners, MAX_LISTENERS)
            };
            match slot {
                Some(slot) => {
                    let connection = ListenerConnection {
                        resources: Some(ConnectionResources::new(sock, slot, lease)),
                        peer: peer_address,
                    };
                    // Required frame storage is admitted before dispatching or sending HTTP 200.
                    // A failed allocation cannot fall back to memory needed by DMA or stacks.
                    if !network.is_current(network_lease) {
                        return;
                    }
                    let buffer = match ListenerBuffer::try_new(format) {
                        Ok(buffer) => buffer,
                        Err(error) => {
                            listener_telemetry::send_admitted_refusal(
                                connection.socket(),
                                &EVENTS,
                                connection.peer,
                                RefusalReason::BufferAllocation(ErrorCode::from(&error)),
                                HEAD_TIMEOUT,
                                &Status::ServiceUnavailable.response(),
                                event_us,
                            );
                            return;
                        }
                    };
                    if !network.is_current(network_lease) {
                        return;
                    }
                    let job = ListenerJob {
                        connection,
                        ring: ring.clone(),
                        buffer,
                        preset,
                        policy,
                        network: network.clone(),
                        network_lease,
                    };
                    if let Err(job) = outputs.try_submit(job) {
                        send_refusal(
                            job.connection.socket(),
                            &peer,
                            &request_line,
                            Status::ServiceUnavailable,
                        );
                        // Rejection returns ownership; checked close runs outside the mailbox lock.
                        drop(job);
                        log::error!("stream: output mailbox rejected admitted listener");
                    }
                }
                None => refuse(sock, &peer, &request_line, Status::ServiceUnavailable),
            }
        }
        Route::Refuse(status) => refuse(
            sock,
            &peer,
            &request_line,
            stream_http::reservation_refusal(status, sonos_intent.is_some()),
        ),
    }
}

/// Sends `status`'s complete response, then closes the connection.
fn refuse(sock: TcpStream, peer: &str, request_line: &str, status: Status) {
    send_refusal(&sock, peer, request_line, status);
}

/// Sends a refusal while the caller retains socket ownership and its close policy.
fn send_refusal(sock: &TcpStream, peer: &str, request_line: &str, status: Status) {
    let response = status.response();
    let status_line = response.lines().next().unwrap_or_default();
    log::info!("stream: {peer} {request_line:?}: {status_line}");
    // A client that never reads can't hold the accept thread for long.
    let sent = stream_http::write_response(sock, response.as_bytes(), HEAD_TIMEOUT);
    if let Err(e) = sent {
        log::info!("stream: {peer}: response not sent ({e})");
    }
}

/// Streams one job until the listener goes or falls behind, then checked-closes its slot.
fn listen(
    connection: ListenerConnection,
    ring: SharedStreamRing,
    buffer: ListenerBuffer,
    preset: Option<BufferPreset>,
    policy: ListenerPolicy,
    network: Network,
    network_lease: NetworkLease,
) {
    let slot = &connection
        .resources
        .as_ref()
        .expect("listener owns resources")
        .slot;
    let sock = connection.socket();
    let lease = &connection
        .resources
        .as_ref()
        .expect("listener owns resources")
        .lease;
    let peer = connection.peer;
    let started = Instant::now();
    let mut departure = None;
    let end = match buffer {
        ListenerBuffer::Wav => stream_wav(sock, &ring, peer, &network, network_lease, slot),
        ListenerBuffer::Compressed(mut bytes) => {
            let mut metrics = ListenerMetrics::new(
                lease
                    .as_ref()
                    .expect("compressed lease")
                    .epoch
                    .format
                    .block_frames(),
            );
            #[cfg(feature = "listener-frame-state")]
            let mut frame_state = crate::listener_frame_state::register_current(peer);
            let end = stream_compressed(
                sock,
                &lease.as_ref().expect("compressed lease").epoch,
                bytes.as_mut_slice(),
                preset,
                policy,
                peer,
                &mut metrics,
                &network,
                network_lease,
                slot,
                #[cfg(feature = "listener-frame-state")]
                frame_state.as_mut(),
            );
            #[cfg(feature = "listener-frame-state")]
            drop(frame_state);
            departure = Some(metrics.departure());
            end
        }
    };
    // Integer tenths: formatting a float to a precision puts a 1 KB buffer on this small stack.
    let tenths = (started.elapsed().as_millis() / 100) as u64;
    // Lifetime minimum for this permanent output worker, across every job and wait.
    // SAFETY: NULL selects this live worker; IDF reports stack headroom in bytes.
    let owner_stack_free_bytes = unsafe { sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
    EVENTS.publish(Record {
        event_us: event_us(),
        peer,
        payload: Payload::Departure {
            metrics: departure,
            tenths,
            end: EndCode::from(&end),
            owner_stack_free_bytes,
        },
    });
    // The connection holds its slot through blocking close: queued payload gets two seconds to
    // drain, plus lwIP's 500 ms close poll and scheduling delay, before abort reclaims it. On this
    // routed path the final RST may never reach a stalled peer; board reclamation does not depend
    // on its arrival. FIN/TIME_WAIT control PCBs can still outlive successful close, so the slot
    // cap bounds active/draining payloads rather than every TCP object. Failures retain the slot
    // until reboot instead of admitting another listener against uncertain resources.
    drop(connection);
}

/// How long writing one frame may take: as long as the ring holds. A listener that takes longer has
/// stopped reading, and by then the ring has overwritten everything it wasn't sent. The limit holds
/// for the whole frame (`DeadlineWriter`), not for each send of it, so a trickle can't stretch it.
fn frame_write_limit(capacity: usize) -> Duration {
    let capacity = capacity as u64;
    Duration::from_millis(1_000 * capacity / u64::from(CAPTURE_RATE_HZ))
}

/// Copies one committed block fragment within the freshness budget.
/// Brief publication collisions retry before sleeping; rejected scratch never leaves this copy.
fn read_some(
    ring: &SharedStreamRing,
    cursor: &mut Cursor,
    out: &mut [i32],
    mut metrics: Option<&mut RawReadMetrics>,
    slot: &Slot,
) -> Result<esp_shazam_core::stream_ring::RawRead, WaitError<ReadError>> {
    let mut receipt = None;
    let wait_started = metrics.as_ref().map(|_| Instant::now());
    let result = read_atomic_fresh_with(FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP, || {
        if slot.revoked() {
            return Err(ReadError::Overrun);
        }
        // Single-read wall time includes preemption, excludes retry sleeps and socket/encoder work.
        let started = Instant::now();
        let result = ring.read(cursor, out).map(|read| {
            receipt = read.receipt;
            read.frames
        });
        RAW_READ_MAX_US.fetch_max(
            started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32,
            Ordering::Relaxed,
        );
        if let Some(metrics) = metrics.as_deref_mut() {
            metrics.attempts += 1;
            match result {
                Ok(0) => metrics.empty += 1,
                Ok(frames) => {
                    metrics.successes += 1;
                    metrics.frames += frames as u64;
                }
                Err(ReadError::Retry) => metrics.retries += 1,
                Err(ReadError::Overrun) => metrics.overruns += 1,
            }
        }
        result
    });
    if let (Some(metrics), Some(started)) = (metrics, wait_started) {
        let elapsed_us = started.elapsed().as_micros() as u64;
        metrics.calls += 1;
        metrics.wait_total_us += elapsed_us;
        metrics.wait_max_us = metrics.wait_max_us.max(elapsed_us);
    }
    result.map(|frames| esp_shazam_core::stream_ring::RawRead { frames, receipt })
}

/// Sends the response head and the WAV header, then frames from the ring as they arrive, until the
/// listener goes, stops reading, or falls a ring behind.
fn stream_wav(
    sock: &TcpStream,
    ring: &SharedStreamRing,
    peer: Option<SocketAddr>,
    network: &Network,
    lease: NetworkLease,
    slot: &Slot,
) -> End {
    if slot.revoked() {
        return End::Retired;
    }
    if !network.is_current(lease) {
        return End::NetworkChanged;
    }
    let write_limit = frame_write_limit(ring.capacity());
    let Some(mut cursor) = cursor_before(
        ring,
        Instant::now() + FRESH_AUDIO_TIMEOUT,
        CAUGHT_UP_SLEEP,
        || slot.revoked() || !network.is_current(lease),
    ) else {
        return if slot.revoked() {
            End::Retired
        } else if network.is_current(lease) {
            End::CaptureStalled
        } else {
            End::NetworkChanged
        };
    };
    let mut writer = DeadlineWriter::new(sock);
    let mut frames = vec![0i32; 2 * CHUNK_FRAMES];
    let mut bytes = Vec::with_capacity(3 * frames.len());
    bytes.extend_from_slice(Format::Wav.response_head().as_bytes());
    bytes.extend_from_slice(&wav::endless_header_s24le(CAPTURE_RATE_HZ, 2));
    let age = esp_shazam_core::media_age::MediaAge::for_stream(None, OutputFormat::Flac);
    let mut oldest = None;
    let observation = crate::listener_observation::REGISTRY.register(event_us());
    loop {
        if slot.revoked() {
            return End::Retired;
        }
        if !network.is_current(lease) {
            return End::NetworkChanged;
        }
        let at = event_us();
        let write_started_us = event_us();
        if let (Some(owner), Some(oldest)) = (&observation, oldest) {
            owner.begin(write_started_us, oldest);
        }
        let result = if let Some(oldest) = oldest {
            let deadline = match age.deadline(
                at,
                oldest,
                at.saturating_add(write_limit.as_micros() as u64),
            ) {
                Ok(deadline) => deadline,
                Err(_) => return End::MediaAge,
            };
            writer.write_all_before(&bytes, socket_deadline(Duration::from_micros(deadline)))
        } else {
            write_frame(&mut writer, &bytes, write_limit, peer)
        };
        if let (Some(owner), Some(oldest)) = (&observation, oldest) {
            owner.finish(event_us(), write_started_us, oldest, result.is_ok());
        }
        if oldest.is_some_and(|oldest| age.check(event_us(), oldest).is_err()) {
            return End::MediaAge;
        }
        if slot.revoked() {
            return End::Retired;
        }
        if !network.is_current(lease) {
            return End::NetworkChanged;
        }
        if let Err(e) = result {
            return End::from_write_error(e);
        }
        let result = read_some(ring, &mut cursor, &mut frames, None, slot);
        if slot.revoked() {
            return End::Retired;
        }
        if !network.is_current(lease) {
            return End::NetworkChanged;
        }
        let frames_read = match result {
            Ok(read) => {
                oldest = Some(
                    read.receipt
                        .expect("nonempty raw read carries metadata")
                        .oldest_accepted_us,
                );
                read.frames
            }
            Err(WaitError::Stalled | WaitError::Read(ReadError::Retry)) => {
                return End::CaptureStalled
            }
            Err(WaitError::Read(ReadError::Overrun)) => return End::FellBehind,
        };
        wav::pack_s24le(&frames[..2 * frames_read], &mut bytes);
    }
}

/// Reports the real frame-write duration only when its existing absolute deadline times out.
fn write_frame(
    writer: &mut DeadlineWriter<'_>,
    bytes: &[u8],
    limit: Duration,
    peer: Option<SocketAddr>,
) -> io::Result<()> {
    let started = Instant::now();
    let timing = listener_telemetry::WriteTiming::for_instant_limit(limit);
    let result = writer.write_all_before(bytes, started + limit);
    listener_telemetry::report_timeout(
        &EVENTS,
        peer,
        &result,
        started.elapsed().as_millis() as u64,
        timing,
        bytes.len(),
        event_us,
    );
    result
}

/// One encoder converts 24-bit capture to 16-bit PCM and shares the compressed result.
/// A raw overrun discards the partial block and invalidates old encoded cursors: existing
/// listeners disconnect rather than hearing discontinuous audio presented as a continuous stream.
fn encode(
    raw: SharedStreamRing,
    runtime: Arc<Runtime>,
    listeners: Arc<Capacity>,
    mut worker: Worker,
) {
    let mut pump = EncoderPump::new();
    let mut yield_frames = 0usize;
    #[cfg(feature = "encoder-probe")]
    let storage = encoder_telemetry::Storage::snapshot(&raw, &pump, &worker);
    let initial_sample = owner_sample(&listeners, false);
    let mut metrics = EncoderMetrics::new(initial_sample);
    metrics.boundary(
        &encoder_telemetry::EVENTS,
        Reason::Startup,
        initial_sample,
        worker.epoch.format,
        #[cfg(feature = "encoder-probe")]
        Some(storage),
    );
    loop {
        let mut published = false;
        let mut log_due = false;
        #[cfg(feature = "encoder-probe")]
        let gate = metrics.probe.enabled();
        #[cfg(feature = "encoder-probe")]
        pump.set_probe(gate);
        let mut reset = false;
        let step = pump.step(
            &raw,
            &mut worker,
            &runtime,
            &mut || Duration::from_micros(event_us()),
            &mut |observation| match observation {
                EncoderObservation::HealthAck { operator_epoch } => {
                    publish_health_event(
                        crate::stream_runtime::health::State::Recovering,
                        operator_epoch,
                        &runtime,
                        &raw,
                        event_us(),
                        None,
                    );
                }
                EncoderObservation::AcceptedInput {
                    generation,
                    start,
                    frames,
                } => {
                    let _ = (generation, start, frames);
                }
                EncoderObservation::CursorAcquired { generation, start } => {
                    let _ = (generation, start);
                }
                EncoderObservation::RawAttempt { result, elapsed } => {
                    RAW_READ_MAX_US.fetch_max(
                        elapsed.as_micros().min(u128::from(u32::MAX)) as u32,
                        Ordering::Relaxed,
                    );
                    #[cfg(feature = "encoder-probe")]
                    {
                        metrics
                            .probe
                            .attempt(matches!(result, Ok(n) if n > 0), elapsed.as_micros() as u64);
                        if matches!(result, Err(ReadError::Overrun)) {
                            metrics.probe.invalidate();
                        }
                    }
                    metrics.raw_read.attempts += 1;
                    match result {
                        Ok(0) => metrics.raw_read.empty += 1,
                        Ok(frames) => {
                            metrics.raw_read.successes += 1;
                            metrics.raw_read.frames += frames as u64;
                        }
                        Err(ReadError::Retry) => metrics.raw_read.retries += 1,
                        Err(ReadError::Overrun) => metrics.raw_read.overruns += 1,
                    }
                }
                EncoderObservation::RawWait(elapsed) => {
                    let us = elapsed.as_micros() as u64;
                    metrics.raw_read.calls += 1;
                    metrics.raw_read.wait_total_us += us;
                    metrics.raw_read.wait_max_us = metrics.raw_read.wait_max_us.max(us);
                }
                #[cfg(feature = "encoder-probe")]
                EncoderObservation::PcmPush(elapsed) => {
                    metrics.probe.push(elapsed.as_micros() as u64)
                }
                EncoderObservation::Codec(elapsed) => {
                    let us = elapsed.as_micros() as u64;
                    metrics.encode_max_us = metrics.encode_max_us.max(us);
                    metrics.encode_wall_total_us += us;
                }
                EncoderObservation::Publication { lock_wait, held } => {
                    metrics.encoded_lock_wait_wall_total_us += lock_wait.as_micros() as u64;
                    metrics.encoded_push_held_wall_total_us += held.as_micros() as u64;
                    metrics.frames += 1;
                    published = true;
                    log_due = true;
                }
                EncoderObservation::Reset => {
                    metrics.observe_reset();
                    reset = true;
                    log_due = true;
                }
                EncoderObservation::CursorWaitExpired | EncoderObservation::Stalled => {
                    log_due = true;
                }
            },
        );
        let boundary_reason = encoder_telemetry::boundary_reason(&step, reset);
        if let Some(reason) = boundary_reason {
            #[cfg(feature = "encoder-probe")]
            let storage = (reset || reason == Reason::Switched)
                .then(|| encoder_telemetry::Storage::snapshot(&raw, &pump, &worker));
            // Receipt copies precede the fresh owner baseline. Queue publication follows it.
            // Default reset and terminal turns never sampled task runtime before this extraction.
            let sample = if reason == Reason::Stopped
                || (!cfg!(feature = "encoder-probe") && reason == Reason::Overrun)
            {
                Sample {
                    us: event_us(),
                    runtime: 0,
                    listeners: 0,
                    stack_free_bytes: 0,
                }
            } else {
                owner_sample(&listeners, false)
            };
            metrics.boundary(
                &encoder_telemetry::EVENTS,
                reason,
                sample,
                worker.epoch.format,
                #[cfg(feature = "encoder-probe")]
                storage,
            );
        }
        match step {
            EncoderStep::Switched => {
                yield_frames = 0;
                // SAFETY: only this worker changes its own FreeRTOS priority at a block boundary.
                unsafe {
                    sys::vTaskPrioritySet(
                        std::ptr::null_mut(),
                        u32::from(worker.epoch.format.priority()),
                    )
                };
            }
            EncoderStep::SleepUntil(at) => {
                let _ = at;
                // Preserve a full tick: subtracting wrapper overhead can make IDF busy-wait.
                std::thread::sleep(CAUGHT_UP_SLEEP);
            }
            EncoderStep::Spin => std::hint::spin_loop(),
            EncoderStep::Stopped(error) => {
                metrics.failure(&encoder_telemetry::EVENTS, event_us(), &error);
                return;
            }
            EncoderStep::Progress => {}
        }
        crate::stream_pipeline::finish_encoder_turn(
            published,
            worker.epoch.format,
            &mut yield_frames,
            &mut metrics,
            |metrics| {
                if log_due {
                    if event_us() - metrics.since_us >= 10_000_000 {
                        metrics.report(
                            &encoder_telemetry::EVENTS,
                            owner_sample(&listeners, true),
                            worker.epoch.format,
                        );
                    }
                }
            },
            |metrics| {
                #[cfg(not(feature = "encoder-probe"))]
                let _ = metrics;
                #[cfg(feature = "encoder-probe")]
                let started = gate.then(Instant::now);
                // Yield regularly even when capture already has more PCM.
                // A one-tick RTOS delay yields; IDF sub-tick usleep can busy-wait.
                // SAFETY: this code runs on the initialized encoder FreeRTOS task.
                unsafe { sys::vTaskDelay(1) };
                #[cfg(feature = "encoder-probe")]
                if let Some(started) = started {
                    metrics.probe.delay(started.elapsed().as_micros() as u64);
                }
            },
        );
    }
}

/// Local compressed-media observations; task runtime excludes its unfinished scheduling slice.
/// Wall intervals include preemption. Frame counters exclude HTTP/media headers and startup fill.
struct ListenerMetrics {
    block_frames: usize,
    since_us: u64,
    runtime_us: u32,
    frames_sent: u64,
    bytes_sent: u64,
    frame_write_wall_total_us: u64,
    frame_write_wall_max_us: u64,
    frame_read_wait_wall_total_us: u64,
    payload_elapsed_us: u64,
    socket: WriterDiagnostics,
    media: listener_telemetry::ListenerMedia,
}

impl ListenerMetrics {
    fn new(block_frames: usize) -> Self {
        let (since_us, runtime_us) = owner_snapshot();
        Self {
            block_frames,
            since_us,
            runtime_us,
            frames_sent: 0,
            bytes_sent: 0,
            frame_write_wall_total_us: 0,
            frame_write_wall_max_us: 0,
            frame_read_wait_wall_total_us: 0,
            payload_elapsed_us: 0,
            socket: WriterDiagnostics::default(),
            media: listener_telemetry::ListenerMedia::default(),
        }
    }

    fn departure(&self) -> Departure {
        let (now, runtime) = owner_snapshot();
        Departure {
            elapsed_us: now - self.since_us,
            task_runtime_us: runtime.wrapping_sub(self.runtime_us),
            runtime_valid: now <= u64::from(u32::MAX),
            frames_sent: self.frames_sent,
            bytes_sent: self.bytes_sent,
            frame_write_wall_total_us: self.frame_write_wall_total_us,
            frame_write_wall_max_us: self.frame_write_wall_max_us,
            frame_read_wait_wall_total_us: self.frame_read_wait_wall_total_us,
            payload_elapsed_us: self.payload_elapsed_us,
            block_samples: self.block_frames,
            socket: self.socket,
            media: self.media,
        }
    }
}

/// An HTTP header precedes shared whole frames. FLAC adds its media header and listener numbering.
/// The bounded output buffer holds one complete frame, so every socket write shares that frame's
/// single deadline. The ring's mutex is released before any socket call.
fn stream_compressed(
    sock: &TcpStream,
    epoch: &Epoch,
    bytes: &mut [u8],
    preset: Option<BufferPreset>,
    policy: ListenerPolicy,
    peer: Option<SocketAddr>,
    metrics: &mut ListenerMetrics,
    network: &Network,
    network_lease: NetworkLease,
    slot: &Slot,
    #[cfg(feature = "listener-frame-state")] mut frame_state: Option<
        &mut crate::listener_frame_state::Owner<'_>,
    >,
) -> End {
    let mut listener =
        CompressedListener::new(epoch, preset, CONFIG.shazam_enabled, policy, &mut || {
            Duration::from_micros(event_us())
        });
    let mut writer = DeadlineWriter::new(sock);
    let mut payload_started = None;
    let observation = crate::listener_observation::REGISTRY.register(event_us());
    let end = loop {
        if slot.revoked() {
            break End::Retired;
        }
        if !network.is_current(network_lease) {
            break End::NetworkChanged;
        }
        let action = listener.step_network(
            epoch,
            bytes,
            &mut || Duration::from_micros(event_us()),
            &mut || stream_http::peer_connected(sock).unwrap_or(false),
            network,
            network_lease,
        );
        match action {
            ListenerStep::Progress => {}
            ListenerStep::SleepUntil(at) => {
                let _ = at;
                // Preserve a full tick: subtracting wrapper overhead can make IDF busy-wait.
                std::thread::sleep(CAUGHT_UP_SLEEP);
            }
            ListenerStep::End(end) => break end,
            ListenerStep::Write { deadline, header } => {
                if slot.revoked() {
                    break End::Retired;
                }
                if !network.is_current(network_lease) {
                    break End::NetworkChanged;
                }
                let started = Instant::now();
                #[cfg(feature = "listener-frame-state")]
                if !header {
                    if let Some(owner) = frame_state.as_deref_mut() {
                        owner.begin(crate::listener_frame_state::now());
                    }
                }
                let write_started_us = event_us();
                let timing = listener_telemetry::WriteTiming::new(
                    write_started_us,
                    deadline.as_micros() as u64,
                    epoch.format.write_limit_ms(CONFIG.shazam_enabled),
                );
                let oldest_us = listener.pending_oldest_accepted_us();
                if let (Some(owner), Some(oldest)) = (&observation, oldest_us) {
                    owner.begin(write_started_us, oldest);
                }
                let result =
                    writer.write_all_before(listener.write_bytes(bytes), socket_deadline(deadline));
                if let (Some(owner), Some(oldest)) = (&observation, oldest_us) {
                    owner.finish(event_us(), write_started_us, oldest, result.is_ok());
                }
                #[cfg(feature = "listener-frame-state")]
                if !header {
                    if let Some(owner) = frame_state.as_deref_mut() {
                        owner.end();
                    }
                }
                listener_telemetry::report_timeout(
                    &EVENTS,
                    peer,
                    &result,
                    started.elapsed().as_millis() as u64,
                    timing,
                    listener.write_bytes(bytes).len(),
                    event_us,
                );
                if !header {
                    let us = started.elapsed().as_micros() as u64;
                    metrics.frame_write_wall_total_us += us;
                    metrics.frame_write_wall_max_us = metrics.frame_write_wall_max_us.max(us);
                }
                if slot.revoked() {
                    break End::Retired;
                }
                if !network.is_current(network_lease) {
                    break End::NetworkChanged;
                }
                let succeeded = result.is_ok();
                if let ListenerStep::End(end) =
                    listener.write_finished(result, &mut || Duration::from_micros(event_us()))
                {
                    break end;
                }
                if header && succeeded {
                    writer.reset_diagnostics();
                    payload_started = Some(Instant::now());
                }
            }
        }
    };
    if listener.refuse_buffered && network.is_current(network_lease) {
        listener_telemetry::send_admitted_refusal(
            sock,
            &EVENTS,
            peer,
            RefusalReason::BufferedStartup,
            HEAD_TIMEOUT,
            &Status::ServiceUnavailable.response(),
            event_us,
        );
    }
    metrics.frame_read_wait_wall_total_us += listener.read_wait.as_micros() as u64;
    metrics.frames_sent += listener.frames_sent;
    metrics.bytes_sent += listener.bytes_sent;
    if let (Some(first), Some(last)) = (listener.first_sent, listener.last_sent) {
        metrics.media = listener_telemetry::ListenerMedia {
            valid: true,
            first_raw_generation: first.metadata.source.raw_generation,
            last_raw_generation: last.metadata.source.raw_generation,
            source_start: first.metadata.source.source_start,
            source_end: last.metadata.source.source_end,
            first_encoded_generation: first.generation,
            last_encoded_generation: last.generation,
            first_encoded_sequence: first.sequence,
            last_encoded_sequence: last.sequence,
            last_age_us: listener.last_age_us,
            max_age_us: listener.max_age_us,
            queue_frames_max: listener.queue_frames_max,
        };
    }
    metrics.socket = writer.diagnostics();
    metrics.payload_elapsed_us =
        payload_started.map_or(0, |started| started.elapsed().as_micros() as u64);
    end
}

fn owner_snapshot() -> (u64, u32) {
    let mut status = sys::TaskStatus_t::default();
    // SAFETY: NULL selects this live calling task. Stack scanning and scheduler-wide lookup are disabled.
    unsafe {
        sys::vTaskGetInfo(
            std::ptr::null_mut(),
            &mut status,
            0,
            sys::eTaskState_eRunning,
        );
        (sys::esp_timer_get_time() as u64, status.ulRunTimeCounter)
    }
}
fn owner_sample(listeners: &Capacity, scan_stack: bool) -> Sample {
    let (us, runtime) = owner_snapshot();
    let listeners = listeners.held();
    // SAFETY: only the encoder owner calls this; IDF reports its minimum free stack in bytes.
    let stack_free_bytes = if scan_stack {
        unsafe { sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) }
    } else {
        0
    };
    Sample {
        us,
        runtime,
        listeners,
        stack_free_bytes,
    }
}

#[inline(never)]
pub(crate) fn export_media_progress(raw: &esp_shazam_core::stream_ring::StreamRing) {
    let Some(runtime) = RUNTIME.get() else {
        return;
    };
    let accepted = raw.accepted_progress().ok();
    let committed = raw.progress().ok();
    let output = runtime.health.progress();
    let at = event_us();
    let source = output.map(|output| output.receipt.metadata.source);
    let backlog = committed
        .zip(source)
        .filter(|(raw, source)| {
            raw.generation == source.raw_generation && source.source_end <= raw.committed_frames
        })
        .map(|(raw, source)| raw.committed_frames.saturating_sub(source.source_end));
    let operator_epoch = runtime.health.token() >> 2;
    let health = runtime.health.state();
    crate::device_metrics::CACHE.publish_media_progress(
        crate::device_metrics::project_media_progress(
            at,
            operator_epoch,
            health,
            accepted.as_ref(),
            committed.as_ref(),
            output.as_ref(),
            backlog,
        ),
    );
    log::info!("media-progress: event_us={} operator_epoch={} health={:?} accepted_valid={} accepted_generation={} accepted_frames={} accepted_us={} raw_valid={} raw_generation={} raw_committed={} raw_publication_us={} encoded_valid={} encoded_operator_epoch={} encoded_generation={} encoded_sequence={} source_start={} source_end={} oldest_accepted_us={} source_raw_publication_us={} encoded_production_us={} source_age_us={} backlog_valid={} backlog_frames={}",
        at,operator_epoch,health,accepted.is_some(),accepted.map_or(0,|a|a.generation),accepted.map_or(0,|a|a.frames),accepted.and_then(|a|a.at_us).unwrap_or(0),committed.is_some(),committed.map_or(0,|r|r.generation),committed.map_or(0,|r|r.committed_frames),committed.and_then(|r|r.committed_at_us).unwrap_or(0),output.is_some(),output.map_or(0,|o|o.operator_epoch),output.map_or(0,|o|o.receipt.generation),output.map_or(0,|o|o.receipt.sequence),source.map_or(0,|s|s.source_start),source.map_or(0,|s|s.source_end),source.map_or(0,|s|s.oldest_accepted_us),source.map_or(0,|s|s.raw_publication_us),output.map_or(0,|o|o.receipt.metadata.encoded_production_us),source.map_or(0,|s|at.saturating_sub(s.oldest_accepted_us)),backlog.is_some(),backlog.unwrap_or(0));
}

pub(crate) fn supervise_encoder(
    supervisor: &mut crate::stream_runtime::health::Supervisor,
    raw: &esp_shazam_core::stream_ring::StreamRing,
    now_us: u64,
) {
    let Some(runtime) = RUNTIME.get() else {
        return;
    };
    if let Some(transition) = supervisor.observe(runtime, raw, now_us) {
        publish_health_event(
            transition.state,
            transition.operator_epoch,
            runtime,
            raw,
            now_us,
            Some(transition),
        );
    }
}
fn publish_health_event(
    state: crate::stream_runtime::health::State,
    operator_epoch: u32,
    runtime: &Runtime,
    raw: &esp_shazam_core::stream_ring::StreamRing,
    now_us: u64,
    observed: Option<crate::stream_runtime::health::Transition>,
) {
    let (raw_snapshot, output) = match observed {
        Some(transition) => (
            transition
                .observation
                .map(|raw| (raw.raw_generation, raw.raw_committed)),
            transition.output,
        ),
        None => (
            raw.progress()
                .ok()
                .map(|raw| (raw.generation, raw.committed_frames)),
            runtime.health.progress(),
        ),
    };
    let (raw_generation, raw_committed) = raw_snapshot.unwrap_or((0, 0));
    let receipt = encoder_telemetry::HealthTransition {
        transition: state,
        operator_epoch,
        raw_valid: raw_snapshot.is_some(),
        encoded_valid: output.is_some(),
        raw_generation,
        raw_committed,
        encoded_generation: output.map_or(0, |output| output.receipt.generation),
        encoded_sequence: output.map_or(0, |output| output.receipt.sequence),
        source_end: output.map_or(0, |output| output.receipt.metadata.source.source_end),
        encoded_production_us: output
            .map_or(0, |output| output.receipt.metadata.encoded_production_us),
    };
    encoder_telemetry::EVENTS.publish(encoder_telemetry::Record {
        event_us: now_us,
        epoch: u64::from(operator_epoch),
        window: 0,
        payload: encoder_telemetry::Payload::Health(receipt),
    });
}

/// Anchor before reading the boot clock so preemption cannot extend an absolute deadline.
fn socket_deadline(deadline: Duration) -> Instant {
    let anchor = Instant::now();
    let now = Duration::from_micros(event_us());
    anchor + deadline.saturating_sub(now)
}

fn event_us() -> u64 {
    // SAFETY: ESP-IDF is initialized and its monotonic timer is available to every task.
    unsafe { sys::esp_timer_get_time() as u64 }
}

impl std::fmt::Display for EndCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retired => f.write_str("format epoch retired"),
            Self::NetworkChanged => f.write_str("network generation changed"),
            Self::EncodedReset => f.write_str("encoded ring generation changed"),
            Self::InvalidCursor => f.write_str("encoded ring cursor invalid"),
            Self::MediaAge => f.write_str("media age exceeded"),
            Self::EncoderUnhealthy => f.write_str("encoder unhealthy"),
            Self::FellBehind => f.write_str("fell a ring behind"),
            Self::StoppedReading => f.write_str("stopped reading"),
            Self::CaptureStalled => {
                write!(f, "no fresh audio for {} s", FRESH_AUDIO_TIMEOUT.as_secs())
            }
            Self::Failed(error) => write!(
                f,
                "socket error kind={:?} raw_os_error={:?}",
                error.kind, error.raw_os_error
            ),
            Self::InvalidFrame(error) => write!(
                f,
                "compressed frame invalid: kind={:?} raw_os_error={:?}",
                error.kind, error.raw_os_error
            ),
        }
    }
}

/// Called only by the existing metrics owner. Each record leaves the queue before output.
pub(crate) fn export_listener_events() {
    let mut first = None;
    let mut last = None;
    for _ in 0..2 {
        let Some(record) = EVENTS.take() else {
            break;
        };
        first = Some(first.map_or(record.event_us, |value: u64| value.min(record.event_us)));
        last = Some(last.map_or(record.event_us, |value: u64| value.max(record.event_us)));
        let peer = record
            .peer
            .map_or_else(|| "unknown peer".to_owned(), |peer| peer.to_string());
        let event_us = record.event_us;
        match record.payload {
            Payload::Joined { format, held, owner_id, sonos_intent } => log::info!("stream: {peer} joined {} ({held} of {MAX_LISTENERS}) event_us={event_us} owner_id={owner_id} sonos_intent={sonos_intent} role={}", format.path(), if sonos_intent == 0 { "diagnostic" } else { "sonos" }),
            Payload::Closed { elapsed_ms, held, owner_id, sonos_intent } => log::info!("stream: {peer} socket closed in {elapsed_ms} ms; releasing slot ({held} held) event_us={event_us} owner_id={owner_id} sonos_intent={sonos_intent}"),
            Payload::Timeout { elapsed_ms, nominal_limit_ms, remaining_budget_us, deadline_us, bytes } => log::info!("stream: frame write timed out elapsed_ms={elapsed_ms} nominal_limit_ms={nominal_limit_ms} remaining_budget_us={remaining_budget_us} deadline_us={deadline_us:?} bytes={bytes} peer={peer} event_us={event_us}"),
            Payload::Refusal { reason, status, response_error } => {
                let (reason, allocation_error) = match reason {
                    RefusalReason::BufferedStartup => ("BufferedStartup", None),
                    RefusalReason::BufferAllocation(error) => ("BufferAllocation", Some(error)),
                };
                log::info!("stream: {peer} admitted refusal reason={reason} status={status} allocation_error={allocation_error:?} event_us={event_us}");
                if let Some(error) = response_error {
                    log::info!("stream: {peer}: response not sent (kind={:?} raw_os_error={:?}) event_us={event_us}", error.kind, error.raw_os_error);
                }
            }
            Payload::Departure { metrics, tenths, end, owner_stack_free_bytes } => {
                crate::device_metrics::CACHE.publish_last_listener_departure(crate::device_metrics::project_departure(event_us, owner_stack_free_bytes, metrics.as_ref()));
                if let Some(m) = metrics {
                    log::info!("stream-listener: peer={} elapsed_us={} task_runtime_us={} runtime_valid={} frames_sent={} bytes_sent={} frame_write_wall_total_us={} frame_write_wall_max_us={} frame_read_wait_wall_total_us={} block_samples={} listener_stack_free_bytes={} event_us={}",
                        peer, m.elapsed_us, m.task_runtime_us, m.runtime_valid, m.frames_sent, m.bytes_sent,
                        m.frame_write_wall_total_us, m.frame_write_wall_max_us, m.frame_read_wait_wall_total_us,
                        m.block_samples, owner_stack_free_bytes, event_us);
                    let media=&m.media;
                    log::info!("stream-media: peer={} valid={} first_raw_generation={} last_raw_generation={} source_start={} source_end={} first_encoded_generation={} last_encoded_generation={} first_encoded_sequence={} last_encoded_sequence={} last_age_us={} max_age_us={} queue_frames_max={} event_us={}",peer,media.valid,media.first_raw_generation,media.last_raw_generation,media.source_start,media.source_end,media.first_encoded_generation,media.last_encoded_generation,media.first_encoded_sequence,media.last_encoded_sequence,media.last_age_us,media.max_age_us,media.queue_frames_max,event_us);
                    let d = m.socket;
                    log::info!("stream-sender: peer={} payload_elapsed_us={} send_calls={} accepted_bytes={} short_sends={} send_would_block={} send_interrupted={} send_errors={} send_wall_total_us={} send_wall_max_us={} send_over_10ms={} event_us={}",
                        peer, m.payload_elapsed_us, d.send_calls, d.accepted_bytes, d.short_sends,
                        d.send_would_block, d.send_interrupted, d.send_errors, d.send_wall_total_us,
                        d.send_wall_max_us, d.send_over_10ms, event_us);
                    log::info!("stream-sender-poll: peer={} poll_calls={} poll_ready={} poll_timeouts={} poll_interrupted={} poll_errors={} poll_error_ready={} poll_wall_total_us={} poll_wall_max_us={} poll_over_10ms={} ready_to_send_wall_max_us={} ready_then_would_block={} event_us={}",
                        peer, d.poll_calls, d.poll_ready, d.poll_timeouts, d.poll_interrupted,
                        d.poll_errors, d.poll_error_ready, d.poll_wall_total_us, d.poll_wall_max_us,
                        d.poll_over_10ms, d.ready_to_send_wall_max_us, d.ready_then_would_block, event_us);
                }
                log::info!("stream: {peer} left after {}.{} s: {end} listener_stack_free_bytes={owner_stack_free_bytes} event_us={event_us}", tenths / 10, tenths % 10);
            }
        }
    }
    // Emit after each nonempty batch or whenever loss changed, including a poisoned queue.
    static LAST_DROPPED: AtomicU32 = AtomicU32::new(0);
    let dropped = EVENTS.dropped();
    let loss_changed = LAST_DROPPED.swap(dropped, Ordering::Relaxed) != dropped;
    static LOSS_REPORTED: AtomicU32 = AtomicU32::new(0);
    let first_loss = EVENTS.loss_occurred() && LOSS_REPORTED.swap(1, Ordering::Relaxed) == 0;
    if first.is_some() || loss_changed || first_loss {
        // SAFETY: this call runs on the live audio-metrics owner, once per exported batch.
        let stack_free_bytes = unsafe { sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
        let loss_occurred = EVENTS.loss_occurred();
        let observed_device_us = event_us();
        crate::device_metrics::CACHE.publish_listener_loss(crate::device_metrics::Loss {
            observed_device_us,
            dropped,
            loss_occurred,
            metrics_stack_free_bytes: Some(stack_free_bytes),
        });
        log::info!("listener-telemetry: dropped={} loss_occurred={} observed_event_range_valid={} observed_event_min_us={} observed_event_max_us={} record_bytes={} queue_bytes={} metrics_stack_free_bytes={} export_us={} dropped_full={} dropped_contended={} dropped_poisoned={}",
            dropped, loss_occurred, first.is_some(), first.unwrap_or(0), last.unwrap_or(0), std::mem::size_of::<Record>(), std::mem::size_of::<listener_telemetry::Queue>(), stack_free_bytes, observed_device_us, EVENTS.dropped_full(), EVENTS.dropped_contended(), EVENTS.dropped_poisoned());
    }
}

/// Dequeues before rendering; the metrics owner spends at most two records per iteration.
#[inline(never)]
pub(crate) fn export_encoder_events() {
    let (first, last) = export_encoder_records();
    render_encoder_batch(&first, &last);
}

// Release the record workspace before formatting the batch receipt.
#[inline(never)]
fn export_encoder_records() -> (Option<u64>, Option<u64>) {
    use encoder_telemetry::EVENTS;
    let mut first = None;
    let mut last = None;
    for _ in 0..2 {
        let Some(ref record) = EVENTS.take() else {
            break;
        };
        first = Some(first.map_or(record.event_us, |us: u64| us.min(record.event_us)));
        last = Some(last.map_or(record.event_us, |us: u64| us.max(record.event_us)));
        render_encoder_event(record);
    }
    (first, last)
}

// Keep receipt formatting off the dequeue frame that owns the record storage.
#[inline(never)]
fn render_encoder_batch(first: &Option<u64>, last: &Option<u64>) {
    use encoder_telemetry::EVENTS;
    static LAST_DROPPED: AtomicU32 = AtomicU32::new(0);
    static LOSS_REPORTED: AtomicU32 = AtomicU32::new(0);
    let dropped = EVENTS.dropped();
    let changed = LAST_DROPPED.swap(dropped, Ordering::Relaxed) != dropped;
    let first_loss = EVENTS.loss_occurred() && LOSS_REPORTED.swap(1, Ordering::Relaxed) == 0;
    if first.is_some() || changed || first_loss {
        // SAFETY: this is the live metrics owner, and IDF reports its minimum free stack in bytes.
        let stack_free_bytes = unsafe { sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
        let loss_occurred = EVENTS.loss_occurred();
        let observed_device_us = event_us();
        crate::device_metrics::CACHE.publish_encoder_loss(crate::device_metrics::Loss {
            observed_device_us,
            dropped,
            loss_occurred,
            metrics_stack_free_bytes: Some(stack_free_bytes),
        });
        log::info!("encoder-telemetry: dropped={} loss_occurred={} observed_event_range_valid={} observed_event_min_us={} observed_event_max_us={} record_bytes={} queue_bytes={} metrics_stack_free_bytes={} export_us={} dropped_full={} dropped_contended={} dropped_poisoned={}",
            dropped, loss_occurred, first.is_some(), first.unwrap_or(0), last.unwrap_or(0),
            std::mem::size_of::<encoder_telemetry::Record>(), std::mem::size_of::<encoder_telemetry::Queue>(),
            stack_free_bytes, observed_device_us, EVENTS.dropped_full(), EVENTS.dropped_contended(), EVENTS.dropped_poisoned());
    }
}
#[inline(never)]
fn render_encoder_event(record: &encoder_telemetry::Record) {
    if let Some(observation) = crate::device_metrics::project_encoder(record) {
        crate::device_metrics::CACHE.publish_encoder(observation);
    }
    use encoder_telemetry::Payload;
    match &record.payload {
        Payload::Window(window) => {
            render_encoder_window(window);
            #[cfg(feature = "encoder-probe")]
            render_probe_window(window);
        }
        Payload::Boundary(boundary) => {
            render_encoder_boundary(record, boundary);
            #[cfg(feature = "encoder-probe")]
            if let Some(storage) = &boundary.storage {
                render_probe_storage(record, boundary, storage);
            }
            if boundary.reason == Reason::Switched {
                render_encoder_active_format(record, boundary);
            }
        }
        Payload::Failure(error) => render_encoder_failure(record, error),
        Payload::Health(health) => {
            use crate::stream_runtime::health::State;
            let transition = match health.transition {
                State::Healthy => "recovered",
                State::ResetRequested => "reset_requested",
                State::Recovering => "reset_ack",
                State::Terminal => "terminal",
            };
            log::info!("media-health: transition={} event_us={} operator_epoch={} raw_valid={} encoded_valid={} raw_generation={} raw_committed={} encoded_generation={} encoded_sequence={} source_end={} encoded_production_us={}",transition,record.event_us,health.operator_epoch,u8::from(health.raw_valid),u8::from(health.encoded_valid),health.raw_generation,health.raw_committed,health.encoded_generation,health.encoded_sequence,health.source_end,health.encoded_production_us);
        }
    }
}
#[inline(never)]
fn render_encoder_failure(record: &encoder_telemetry::Record, error: &ErrorCode) {
    log::error!("stream: encoder failed; output remains invalid (kind={:?} raw_os_error={:?}) telemetry_epoch={} telemetry_window={} event_us={}", error.kind, error.raw_os_error, record.epoch, record.window, record.event_us);
}
#[inline(never)]
fn render_encoder_window(w: &encoder_telemetry::Window) {
    #[cfg(feature = "encoder-probe")]
    let sequence = w.probe.sequence;
    #[cfg(not(feature = "encoder-probe"))]
    let sequence = 0u64;
    log::info!("stream-encode: elapsed_us={} task_runtime_us={} runtime_valid={} encoded_frames={} resets={} listeners_start={} listeners_end={} (slots include WAV and closing listeners) encode_max_us={} stack_free_bytes={} block_samples={} raw_read_calls={} raw_read_attempts={} raw_read_successes={} raw_read_frames={} raw_read_retries={} raw_read_empty={} raw_read_overruns={} raw_wait_total_us={} raw_wait_max_us={} encode_wall_total_us={} encoded_lock_wait_wall_total_us={} encoded_push_held_wall_total_us={} telemetry_epoch={} telemetry_window={} window_start_us={} window_end_us={} sequence={}",
        w.end_us - w.start_us, w.task_runtime_us, w.runtime_valid, w.frames, w.resets, w.listeners_start, w.listeners_end, w.encode_max_us, w.stack_free_bytes, w.block_samples,
        w.raw_read.calls, w.raw_read.attempts, w.raw_read.successes, w.raw_read.frames, w.raw_read.retries, w.raw_read.empty, w.raw_read.overruns, w.raw_read.wait_total_us, w.raw_read.wait_max_us,
        w.encode_wall_total_us, w.encoded_lock_wait_wall_total_us, w.encoded_push_held_wall_total_us, w.epoch, w.window, w.start_us, w.end_us, sequence);
}
#[cfg(feature = "encoder-probe")]
#[inline(never)]
fn render_probe_window(w: &encoder_telemetry::Window) {
    let r = &w.probe;
    log::info!("encoder-probe: sequence={} phase={} interval={} retained={} completed={} valid={} success_attempt_us={} rejected_attempt_us={} pcm_push_us={} delay_us={} delay_calls={} raw_remainder_us={:?} residual_us={:?} telemetry_epoch={} telemetry_window={} window_start_us={} window_end_us={}",
        r.sequence, r.phase, r.interval, r.retained, r.completed, r.valid, r.success_us, r.rejected_us, r.push_us, r.delay_us, r.delay_calls, r.raw_remainder_us, r.residual_us, w.epoch, w.window, w.start_us, w.end_us);
}
#[inline(never)]
fn render_encoder_boundary(record: &encoder_telemetry::Record, b: &encoder_telemetry::Boundary) {
    log::info!("encoder-boundary: prior_epoch={} reason={:?} frames={} resets={} raw_overruns={} raw_wait_us={} raw_attempts={} telemetry_epoch={} telemetry_window={} event_us={} baseline_us={} format={:?}",
        b.prior_epoch, b.reason, b.frames, b.resets, b.raw_overruns, b.raw_wait_us, b.raw_attempts, record.epoch, record.window, record.event_us, b.baseline_us, b.format);
    if b.reason == Reason::Overrun {
        log::warn!("stream: encoder fell behind capture; disconnecting compressed listeners telemetry_epoch={} telemetry_window={} event_us={}", record.epoch, record.window, record.event_us);
    }
    #[cfg(feature = "encoder-probe")]
    {
        if b.reason != Reason::Startup {
            log::warn!("encoder-probe: invalidated position={:?} frames={} resets={} raw_overruns={} raw_wait_us={} raw_attempts={} reason={:?} telemetry_epoch={} telemetry_window={} event_us={}",
                b.position, b.frames, b.resets, b.raw_overruns, b.raw_wait_us, b.raw_attempts, b.reason, b.prior_epoch, record.window, record.event_us);
        }
    }
}
#[inline(never)]
fn render_encoder_active_format(
    record: &encoder_telemetry::Record,
    b: &encoder_telemetry::Boundary,
) {
    log::info!("stream: active format {:?} telemetry_epoch={} telemetry_window={} event_us={} baseline_us={}", b.format, record.epoch, record.window, record.event_us, b.baseline_us);
}
#[cfg(feature = "encoder-probe")]
#[inline(never)]
fn render_probe_storage(
    record: &encoder_telemetry::Record,
    b: &encoder_telemetry::Boundary,
    s: &encoder_telemetry::Storage,
) {
    log::info!("encoder-probe-storage: sequence={} baseline_pending=true raw_payload={:?} raw_stamps={:?} chunk={:?} output_address={} output_len={} output_capacity={} output_required={} shine_policy=MALLOC_CAP_SPIRAM|MALLOC_CAP_8BIT mp3={:?} telemetry_epoch={} telemetry_window={} event_us={} baseline_us={}",
        b.sequence, s.raw_payload, s.raw_stamps, s.chunk, s.output_address, s.output_len, s.output_capacity, s.output_required, s.mp3, record.epoch, record.window, record.event_us, b.baseline_us);
}
