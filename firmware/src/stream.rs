// ABOUTME: The LINE IN stream on port 8000: serves the selected compressed codec and diagnostic 24-bit WAV at 48 kHz stereo.
// ABOUTME: One core 1 encoder shares compressed frames; listeners copy out before socket work and drop if they fall behind.

use std::io::{self, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

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
    self, DeadlineReader, DeadlineWriter, Format, Route, Status, WriterDiagnostics,
};
use esp_shazam_core::stream_ring::{Cursor, ReadError};
use esp_shazam_core::stream_wait::{cursor_before, read_atomic_fresh_with, Slot, WaitError};
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
static LISTENERS: OnceLock<Arc<AtomicUsize>> = OnceLock::new();
static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();

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
        let listeners = LISTENERS
            .get()
            .ok_or_else(|| io::Error::other("stream listener capacity unavailable"))?;
        runtime.play_format(
            format,
            token,
            listeners,
            MAX_LISTENERS,
            Duration::from_secs(12),
        )
    } else {
        runtime.switch(format, false, token, Duration::from_secs(3))
    }
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

/// Keeps the listener slot until its socket close succeeds, including spawn failure.
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
/// thread, TCP buffers that prefer PSRAM, and Wi-Fi: 2.3 Mbit/s for the WAV, less for compressed audio.
const MAX_LISTENERS: usize = 2;
// Both compressed codecs encode the capture sample rate.
const _: () = assert!(codec::SAMPLE_RATE_HZ == CAPTURE_RATE_HZ);
/// A client gets this long to send its whole request head, which may be at most `MAX_HEAD_BYTES`,
/// and to take a refusal. The accept thread answers one connection at a time, so this is how long
/// one client can hold up the next.
const HEAD_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_HEAD_BYTES: u64 = 4096;
/// The server threads share core 1 with the encoder. Their priority, 7, is
/// above the recognizer's (5), so a fingerprint can't starve a listener, and below the audio thread
/// (10), lwIP (18) and Wi-Fi (23).
const ACCEPT_THREAD: ThreadSpec = ThreadSpec {
    name: c"stream",
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

/// Listens on port 8000 on a thread of its own, and advertises the stream over mDNS as
/// "esp-shazam stream", next to the page's "esp-shazam".
pub fn start(ring: SharedStreamRing, mdns: &mut EspMdns) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", PORT))?;
    let (runtime, worker) = Runtime::new(OutputFormat::Flac, CONFIG.shazam_enabled)?;
    RUNTIME
        .set(runtime.clone())
        .map_err(|_| anyhow::anyhow!("stream already started"))?;
    let listeners = Arc::new(AtomicUsize::new(0));
    let _ = LISTENERS.set(listeners.clone());
    let encoder_listeners = listeners.clone();
    let (encoder_raw, encoder_runtime) = (ring.clone(), runtime.clone());
    ENCODER_THREAD
        .spawn(move || encode(encoder_raw, encoder_runtime, encoder_listeners, worker))?;
    ACCEPT_THREAD.spawn(move || accept(listener, ring, runtime, listeners))?;
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
    log::info!("stream: listening on port {PORT}");
    Ok(())
}

fn accept(
    listener: TcpListener,
    ring: SharedStreamRing,
    runtime: Arc<Runtime>,
    listeners: Arc<AtomicUsize>,
) {
    for conn in listener.incoming() {
        match conn {
            Ok(sock) => answer(sock, &ring, &runtime, &listeners),
            Err(e) => {
                log::warn!("stream: accept failed: {e}");
                // lwIP drops a connection it has no socket for. Pause in case an error repeats at once.
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Reads one request and answers it: a listener thread for a stream, or a refusal.
fn answer(
    sock: TcpStream,
    ring: &SharedStreamRing,
    runtime: &Arc<Runtime>,
    listeners: &Arc<AtomicUsize>,
) {
    let peer_address = sock.peer_addr().ok();
    let peer = peer_address.map_or_else(|| "unknown peer".to_owned(), |addr| addr.to_string());
    // One deadline for the whole head: a per-read timeout would let a client that sends a byte
    // every 1.9 s hold the accept thread for hours.
    let reader = DeadlineReader::new(&sock, Instant::now() + HEAD_TIMEOUT);
    let head =
        stream_http::read_request_line(BufReader::with_capacity(256, reader), MAX_HEAD_BYTES);
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
            match Slot::take(listeners, MAX_LISTENERS) {
                Some(slot) => {
                    let connection = ListenerConnection {
                        resources: Some(ConnectionResources::new(sock, slot, lease)),
                        peer: peer_address,
                    };
                    // Required frame storage is admitted before spawning or sending HTTP 200.
                    // A failed allocation cannot fall back to memory needed by DMA or stacks.
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
                    EVENTS.publish(Record {
                        event_us: event_us(),
                        peer: connection.peer,
                        payload: Payload::Joined {
                            format,
                            held: connection.resources.as_ref().unwrap().slot.taken(),
                        },
                    });
                    let ring = ring.clone();
                    // The owned connection's drop observes close even if no thread can be created.
                    if let Err(e) =
                        LISTENER_THREAD.spawn(move || listen(connection, ring, buffer, preset))
                    {
                        log::warn!("stream: no thread for a listener: {e}");
                    }
                }
                None => refuse(sock, &peer, &request_line, Status::ServiceUnavailable),
            }
        }
        Route::Refuse(status) => refuse(sock, &peer, &request_line, status),
    }
}

/// Sends `status`'s complete response, then closes the connection.
fn refuse(sock: TcpStream, peer: &str, request_line: &str, status: Status) {
    send_refusal(&sock, peer, request_line, status);
}

/// Sends a refusal while the caller retains socket ownership and its close policy.
fn send_refusal(mut sock: &TcpStream, peer: &str, request_line: &str, status: Status) {
    let response = status.response();
    let status_line = response.lines().next().unwrap_or_default();
    log::info!("stream: {peer} {request_line:?}: {status_line}");
    // A client that never reads can't hold the accept thread for long.
    let sent = sock
        .set_write_timeout(Some(HEAD_TIMEOUT))
        .and_then(|()| sock.write_all(response.as_bytes()));
    if let Err(e) = sent {
        log::info!("stream: {peer}: response not sent ({e})");
    }
}

/// A listener's thread: streams `format` until the listener goes or falls behind, then frees its slot.
fn listen(
    connection: ListenerConnection,
    ring: SharedStreamRing,
    buffer: ListenerBuffer,
    preset: Option<BufferPreset>,
) {
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
        ListenerBuffer::Wav => stream_wav(sock, &ring, peer),
        ListenerBuffer::Compressed(mut bytes) => {
            let mut metrics = ListenerMetrics::new(
                lease
                    .as_ref()
                    .expect("compressed lease")
                    .epoch
                    .format
                    .block_frames(),
            );
            let end = stream_compressed(
                sock,
                &lease.as_ref().expect("compressed lease").epoch,
                bytes.as_mut_slice(),
                preset,
                peer,
                &mut metrics,
            );
            departure = Some(metrics.departure());
            end
        }
    };
    // Integer tenths: formatting a float to a precision puts a 1 KB buffer on this small stack.
    let tenths = (started.elapsed().as_millis() / 100) as u64;
    // SAFETY: NULL selects this live listener owner; IDF reports stack headroom in bytes.
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
) -> Result<usize, WaitError<ReadError>> {
    let wait_started = metrics.as_ref().map(|_| Instant::now());
    let result = read_atomic_fresh_with(FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP, || {
        // Single-read wall time includes preemption, excludes retry sleeps and socket/encoder work.
        let started = Instant::now();
        let result = ring.read(cursor, out);
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
    result
}

/// Sends the response head and the WAV header, then frames from the ring as they arrive, until the
/// listener goes, stops reading, or falls a ring behind.
fn stream_wav(sock: &TcpStream, ring: &SharedStreamRing, peer: Option<SocketAddr>) -> End {
    let write_limit = frame_write_limit(ring.capacity());
    let Some(mut cursor) =
        cursor_before(ring, Instant::now() + FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP)
    else {
        return End::CaptureStalled;
    };
    let mut writer = DeadlineWriter::new(sock);
    let mut frames = vec![0i32; 2 * CHUNK_FRAMES];
    let mut bytes = Vec::with_capacity(3 * frames.len());
    bytes.extend_from_slice(Format::Wav.response_head().as_bytes());
    bytes.extend_from_slice(&wav::endless_header_s24le(CAPTURE_RATE_HZ, 2));
    loop {
        if let Err(e) = write_frame(&mut writer, &bytes, write_limit, peer) {
            return End::from_write_error(e);
        }
        let frames_read = match read_some(ring, &mut cursor, &mut frames, None) {
            Ok(n) => n,
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
    let result = writer.write_all_before(bytes, started + limit);
    listener_telemetry::report_timeout(
        &EVENTS,
        peer,
        &result,
        started.elapsed().as_millis() as u64,
        limit.as_millis() as u64,
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
    listeners: Arc<AtomicUsize>,
    mut worker: Worker,
) {
    let mut pump = EncoderPump::new();
    let mut yield_frames = 0usize;
    #[cfg(feature = "encoder-probe")]
    log_probe_storage(
        &raw,
        &pump,
        &worker,
        crate::encoder_probe::Probe::default()
            .next_sequence(worker.epoch.format == OutputFormat::Mp3),
    );
    let mut metrics = EncoderMetrics::new(&listeners);
    #[cfg(feature = "encoder-probe")]
    metrics
        .probe
        .restart(worker.epoch.format == OutputFormat::Mp3);
    let origin = Instant::now();
    loop {
        let mut published = false;
        let mut log_due = false;
        #[cfg(feature = "encoder-probe")]
        let gate = metrics.probe.enabled();
        #[cfg(feature = "encoder-probe")]
        pump.set_probe(gate);
        #[cfg(feature = "encoder-probe")]
        let mut reset = false;
        #[cfg(feature = "encoder-probe")]
        let mut overrun = false;
        let step = pump.step(
            &raw,
            &mut worker,
            &runtime,
            &mut || origin.elapsed(),
            &mut |observation| match observation {
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
                            overrun = true;
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
                    log::warn!(
                        "stream: encoder fell behind capture; disconnecting compressed listeners"
                    );
                    metrics.resets += 1;
                    #[cfg(feature = "encoder-probe")]
                    {
                        reset = true;
                    }
                    log_due = true;
                }
                EncoderObservation::CursorWaitExpired | EncoderObservation::Stalled => {
                    log_due = true;
                }
            },
        );
        #[cfg(feature = "encoder-probe")]
        if overrun || matches!(step, EncoderStep::Switched | EncoderStep::Stopped(_)) {
            metrics.probe.invalidate();
            log::warn!("encoder-probe: invalidated position={:?} frames={} resets={} raw_overruns={} raw_wait_us={} raw_attempts={} state={:?}",
                metrics.probe.position(), metrics.frames, metrics.resets, metrics.raw_read.overruns, metrics.raw_read.wait_total_us,
                metrics.raw_read.attempts, metrics.probe);
        }
        #[cfg(feature = "encoder-probe")]
        if reset || matches!(step, EncoderStep::Switched) {
            log_probe_storage(
                &raw,
                &pump,
                &worker,
                metrics
                    .probe
                    .next_sequence(worker.epoch.format == OutputFormat::Mp3),
            );
            metrics.fresh_probe_baseline(&listeners, worker.epoch.format == OutputFormat::Mp3);
        }
        match step {
            EncoderStep::Switched => {
                yield_frames = 0;
                #[cfg(not(feature = "encoder-probe"))]
                {
                    metrics = EncoderMetrics::new(&listeners);
                }
                // SAFETY: only this worker changes its own FreeRTOS priority at a block boundary.
                unsafe {
                    sys::vTaskPrioritySet(
                        std::ptr::null_mut(),
                        u32::from(worker.epoch.format.priority()),
                    )
                };
                log::info!("stream: active format {:?}", worker.epoch.format);
            }
            EncoderStep::SleepUntil(at) => {
                let _ = at;
                // Preserve a full tick: subtracting wrapper overhead can make IDF busy-wait.
                std::thread::sleep(CAUGHT_UP_SLEEP);
            }
            EncoderStep::Spin => std::hint::spin_loop(),
            EncoderStep::Stopped(error) => {
                log::error!("stream: encoder failed; output remains invalid ({error})");
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
                    metrics.log_if_due(&listeners);
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

/// Worker-local read observations reset after each report; WAV does not contribute.
#[derive(Default)]
struct RawReadMetrics {
    calls: u64,
    attempts: u64,
    successes: u64,
    frames: u64,
    retries: u64,
    empty: u64,
    overruns: u64,
    wait_total_us: u64,
    wait_max_us: u64,
}

/// Task runtime accounting excludes its current scheduling slice and includes interrupt/logging
/// overhead charged to this task. Compare stable listener windows, not encode-call wall latency.
struct EncoderMetrics {
    since_us: u64,
    runtime_us: u32,
    listeners: usize,
    frames: u64,
    resets: u64,
    encode_max_us: u64,
    encode_wall_total_us: u64,
    encoded_lock_wait_wall_total_us: u64,
    encoded_push_held_wall_total_us: u64,
    raw_read: RawReadMetrics,
    #[cfg(feature = "encoder-probe")]
    probe: crate::encoder_probe::Probe,
}

impl EncoderMetrics {
    fn new(listeners: &AtomicUsize) -> Self {
        let (since_us, runtime_us) = Self::snapshot();
        Self {
            since_us,
            runtime_us,
            listeners: listeners.load(std::sync::atomic::Ordering::Acquire),
            frames: 0,
            resets: 0,
            encode_max_us: 0,
            encode_wall_total_us: 0,
            encoded_lock_wait_wall_total_us: 0,
            encoded_push_held_wall_total_us: 0,
            raw_read: RawReadMetrics::default(),
            #[cfg(feature = "encoder-probe")]
            probe: crate::encoder_probe::Probe::default(),
        }
    }

    #[cfg(feature = "encoder-probe")]
    fn fresh_probe_baseline(&mut self, listeners: &AtomicUsize, mp3: bool) {
        let frames = self.frames;
        let resets = self.resets;
        let probe = std::mem::take(&mut self.probe);
        *self = Self::new(listeners);
        self.frames = frames;
        self.resets = resets;
        self.probe = probe;
        self.probe.restart(mp3);
    }

    fn snapshot() -> (u64, u32) {
        let mut status = sys::TaskStatus_t::default();
        // SAFETY: NULL selects this live calling task. IDF initializes the status; pdFALSE
        // skips stack scanning, and eRunning avoids task-state lookup. No scheduler-wide scan.
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

    fn log_if_due(&mut self, listeners: &AtomicUsize) -> bool {
        // SAFETY: IDF initializes the monotonic timer before the encoder task starts.
        let now = unsafe { sys::esp_timer_get_time() as u64 };
        if now - self.since_us < 10_000_000 {
            return false;
        }
        let (now, runtime) = Self::snapshot();
        let current = listeners.load(std::sync::atomic::Ordering::Acquire);
        // SAFETY: NULL selects only this live task; IDF reports its lifetime minimum free stack
        // in bytes. Scan at the ten-second reporting interval, never once per encoded frame.
        let stack_free_bytes = unsafe { sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
        // U32 runtime stats lose accounting at the timer wrap (~71.6 min). Mark later
        // windows invalid rather than presenting wrapping arithmetic as accurate CPU use.
        log::info!("stream-encode: elapsed_us={} task_runtime_us={} runtime_valid={} encoded_frames={} resets={} listeners_start={} listeners_end={} (slots include WAV and closing listeners) encode_max_us={} stack_free_bytes={} block_samples={} raw_read_calls={} raw_read_attempts={} raw_read_successes={} raw_read_frames={} raw_read_retries={} raw_read_empty={} raw_read_overruns={} raw_wait_total_us={} raw_wait_max_us={} encode_wall_total_us={} encoded_lock_wait_wall_total_us={} encoded_push_held_wall_total_us={}",
            now - self.since_us, runtime.wrapping_sub(self.runtime_us), now <= u64::from(u32::MAX),
            self.frames, self.resets, self.listeners, current, self.encode_max_us, stack_free_bytes, active_format().block_frames(),
            self.raw_read.calls, self.raw_read.attempts, self.raw_read.successes,
            self.raw_read.frames, self.raw_read.retries, self.raw_read.empty,
            self.raw_read.overruns, self.raw_read.wait_total_us, self.raw_read.wait_max_us,
            self.encode_wall_total_us, self.encoded_lock_wait_wall_total_us, self.encoded_push_held_wall_total_us);
        #[cfg(feature = "encoder-probe")]
        {
            let report = self.probe.report(
                now - self.since_us,
                self.raw_read.wait_total_us,
                self.encode_wall_total_us,
                self.encoded_lock_wait_wall_total_us,
                self.encoded_push_held_wall_total_us,
            );
            log::info!("encoder-probe: sequence={} phase={} interval={} retained={} completed={} valid={} success_attempt_us={} rejected_attempt_us={} pcm_push_us={} delay_us={} delay_calls={} raw_remainder_us={:?} residual_us={:?}",
                report.sequence, report.phase, report.interval, report.retained, report.completed, report.valid,
                report.success_us, report.rejected_us, report.push_us, report.delay_us,
                report.delay_calls, report.raw_remainder_us, report.residual_us);
        }
        self.since_us = now;
        self.runtime_us = runtime;
        self.listeners = current;
        self.encode_max_us = 0;
        self.encode_wall_total_us = 0;
        self.encoded_lock_wait_wall_total_us = 0;
        self.encoded_push_held_wall_total_us = 0;
        self.raw_read = RawReadMetrics::default();
        true
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
}

impl ListenerMetrics {
    fn new(block_frames: usize) -> Self {
        let (since_us, runtime_us) = EncoderMetrics::snapshot();
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
        }
    }

    fn departure(&self) -> Departure {
        let (now, runtime) = EncoderMetrics::snapshot();
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
    peer: Option<SocketAddr>,
    metrics: &mut ListenerMetrics,
) -> End {
    let origin = Instant::now();
    let mut listener = CompressedListener::new(epoch, preset, CONFIG.shazam_enabled, &mut || {
        origin.elapsed()
    });
    let mut writer = DeadlineWriter::new(sock);
    let mut payload_started = None;
    let end = loop {
        let action = listener.step(epoch, bytes, &mut || origin.elapsed(), &mut || {
            stream_http::peer_connected(sock).unwrap_or(false)
        });
        match action {
            ListenerStep::Progress => {}
            ListenerStep::SleepUntil(at) => {
                let _ = at;
                // Preserve a full tick: subtracting wrapper overhead can make IDF busy-wait.
                std::thread::sleep(CAUGHT_UP_SLEEP);
            }
            ListenerStep::End(end) => break end,
            ListenerStep::Write { deadline, header } => {
                let started = Instant::now();
                let result =
                    writer.write_all_before(listener.write_bytes(bytes), origin + deadline);
                listener_telemetry::report_timeout(
                    &EVENTS,
                    peer,
                    &result,
                    started.elapsed().as_millis() as u64,
                    epoch.format.write_limit_ms(CONFIG.shazam_enabled),
                    listener.write_bytes(bytes).len(),
                    event_us,
                );
                if !header {
                    let us = started.elapsed().as_micros() as u64;
                    metrics.frame_write_wall_total_us += us;
                    metrics.frame_write_wall_max_us = metrics.frame_write_wall_max_us.max(us);
                }
                let succeeded = result.is_ok();
                if let ListenerStep::End(end) = listener.write_finished(result) {
                    break end;
                }
                if header && succeeded {
                    writer.reset_diagnostics();
                    payload_started = Some(Instant::now());
                }
            }
        }
    };
    if listener.refuse_buffered {
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
    metrics.socket = writer.diagnostics();
    metrics.payload_elapsed_us =
        payload_started.map_or(0, |started| started.elapsed().as_micros() as u64);
    end
}

#[cfg(feature = "encoder-probe")]
fn log_probe_storage(raw: &SharedStreamRing, pump: &EncoderPump, worker: &Worker, sequence: u64) {
    log::info!("encoder-probe-storage: sequence={} baseline_pending=true raw_payload={:?} raw_stamps={:?} chunk={:?} output_address={} output_len={} output_capacity={} output_required={} shine_policy=MALLOC_CAP_SPIRAM|MALLOC_CAP_8BIT mp3={:?}",
        sequence, raw.payload_range(), raw.stamps_range(), pump.storage_receipt(),
        worker.bytes.as_ptr() as usize, worker.bytes.len(), worker.bytes.capacity(),
        worker.epoch.format.max_frame_bytes(), worker.encoder.mp3_storage_receipt());
    assert!(worker.bytes.capacity() >= worker.epoch.format.max_frame_bytes());
}

fn event_us() -> u64 {
    // SAFETY: ESP-IDF is initialized and its monotonic timer is available to every task.
    unsafe { sys::esp_timer_get_time() as u64 }
}

impl std::fmt::Display for EndCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retired => f.write_str("format epoch retired"),
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
            Payload::Joined { format, held } => log::info!("stream: {peer} joined {} ({held} of {MAX_LISTENERS}) event_us={event_us}", format.path()),
            Payload::Closed { elapsed_ms, held } => log::info!("stream: {peer} socket closed in {elapsed_ms} ms; releasing slot ({held} held) event_us={event_us}"),
            Payload::Timeout { elapsed_ms, limit_ms, bytes } => log::info!("stream: frame write timed out elapsed_ms={elapsed_ms} limit_ms={limit_ms} bytes={bytes} peer={peer} event_us={event_us}"),
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
                if let Some(m) = metrics {
                    log::info!("stream-listener: peer={} elapsed_us={} task_runtime_us={} runtime_valid={} frames_sent={} bytes_sent={} frame_write_wall_total_us={} frame_write_wall_max_us={} frame_read_wait_wall_total_us={} block_samples={} listener_stack_free_bytes={} event_us={}",
                        peer, m.elapsed_us, m.task_runtime_us, m.runtime_valid, m.frames_sent, m.bytes_sent,
                        m.frame_write_wall_total_us, m.frame_write_wall_max_us, m.frame_read_wait_wall_total_us,
                        m.block_samples, owner_stack_free_bytes, event_us);
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
        log::info!("listener-telemetry: dropped={} loss_occurred={} observed_event_range_valid={} observed_event_min_us={} observed_event_max_us={} record_bytes={} queue_bytes={} metrics_stack_free_bytes={} export_us={} dropped_full={} dropped_contended={} dropped_poisoned={}",
            dropped, EVENTS.loss_occurred(), first.is_some(), first.unwrap_or(0), last.unwrap_or(0), std::mem::size_of::<Record>(), std::mem::size_of::<listener_telemetry::Queue>(), stack_free_bytes, event_us(), EVENTS.dropped_full(), EVENTS.dropped_contended(), EVENTS.dropped_poisoned());
    }
}
