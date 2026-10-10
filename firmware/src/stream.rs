// ABOUTME: The LINE IN stream on port 8000: serves the selected compressed codec and diagnostic 24-bit WAV at 48 kHz stereo.
// ABOUTME: One core 1 encoder shares compressed frames; listeners copy out before socket work and drop if they fall behind.

use std::fmt;
use std::io::{self, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::stream_codec as codec;
use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::mdns::EspMdns;
use esp_idf_svc::sys;
use esp_shazam_core::stream_buffer::{BufferPreset, OutputFormat};
use esp_shazam_core::stream_http::{
    self, DeadlineReader, DeadlineWriter, Format, Route, Status, WriterDiagnostics,
};
use esp_shazam_core::stream_ring::{Cursor, ReadError};
use esp_shazam_core::stream_wait::{
    buffered_cursor_before, cursor_before, lock_before, read_atomic_fresh_with, read_locked_fresh,
    BufferWaitError, Slot, WaitError,
};
use esp_shazam_core::wav;

use crate::audio_in::{SharedStreamRing, CAPTURE_RATE_HZ};
use crate::config::CONFIG;
use crate::psram::PsramBytes;
use crate::socket_close;
use crate::threads::ThreadSpec;

use crate::stream_runtime::{Epoch, Lease, Runtime, Worker};
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
    sock: Option<TcpStream>,
    slot: Slot,
    peer: String,
}

impl ListenerConnection {
    fn socket(&self) -> &TcpStream {
        self.sock.as_ref().expect("listener owns its socket")
    }
}

impl Drop for ListenerConnection {
    fn drop(&mut self) {
        if let Some(sock) = self.sock.take() {
            match socket_close::close(sock) {
                Ok(report) => log::info!(
                    "stream: {} socket closed in {} ms; releasing slot ({} held)",
                    self.peer,
                    report.elapsed.as_millis(),
                    self.slot.taken()
                ),
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
/// Frames per atomic capture read: at most 5 ms, without holding a capture mutex.
/// WAV packs these samples into 1,440 bytes; the shared encoder accumulates codec-sized blocks.
const CHUNK_FRAMES: usize = 240;
// Both compressed codecs encode the capture sample rate.
const _: () = assert!(codec::SAMPLE_RATE_HZ == CAPTURE_RATE_HZ);
/// A listener that has caught up sleeps one FreeRTOS tick: ESP-IDF's usleep busy-waits anything shorter.
const CAUGHT_UP_SLEEP: Duration = Duration::from_millis(10);
/// A caught-up listener waits at most 2 s for real audio, plus one tick and scheduling delay.
/// Quiet audio still produces samples/encoded frames. Capture stalls release both listener slots
/// independently of the capture supervisor; the shared encoder waits again after this timeout.
const FRESH_AUDIO_TIMEOUT: Duration = Duration::from_secs(2);
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
    let peer = sock
        .peer_addr()
        .map_or_else(|_| "unknown peer".to_owned(), |addr| addr.to_string());
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
                        sock: Some(sock),
                        slot,
                        peer,
                    };
                    // Required frame storage is admitted before spawning or sending HTTP 200.
                    // A failed allocation cannot fall back to memory needed by DMA or stacks.
                    let buffer = match ListenerBuffer::try_new(format) {
                        Ok(buffer) => buffer,
                        Err(error) => {
                            log::warn!(
                                "stream: {}: listener buffer unavailable ({error})",
                                connection.peer
                            );
                            send_refusal(
                                connection.socket(),
                                &connection.peer,
                                &request_line,
                                Status::ServiceUnavailable,
                            );
                            return;
                        }
                    };
                    log::info!(
                        "stream: {} joined {} ({} of {MAX_LISTENERS})",
                        connection.peer,
                        format.path(),
                        connection.slot.taken()
                    );
                    let ring = ring.clone();
                    // The owned connection's drop observes close even if no thread can be created.
                    if let Err(e) = LISTENER_THREAD
                        .spawn(move || listen(connection, ring, lease, buffer, preset))
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
    lease: Option<Lease>,
    buffer: ListenerBuffer,
    preset: Option<BufferPreset>,
) {
    let sock = connection.socket();
    let peer = &connection.peer;
    let started = Instant::now();
    let end = match buffer {
        ListenerBuffer::Wav => stream_wav(sock, &ring),
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
                &mut metrics,
            );
            metrics.log_departure(peer);
            end
        }
    };
    // Integer tenths: formatting a float to a precision puts a 1 KB buffer on this small stack.
    let tenths = started.elapsed().as_millis() / 100;
    log::info!(
        "stream: {peer} left after {}.{} s: {end}",
        tenths / 10,
        tenths % 10
    );
    // The connection holds its slot through blocking close: queued payload gets two seconds to
    // drain, plus lwIP's 500 ms close poll and scheduling delay, before abort reclaims it. On this
    // routed path the final RST may never reach a stalled peer; board reclamation does not depend
    // on its arrival. FIN/TIME_WAIT control PCBs can still outlive successful close, so the slot
    // cap bounds active/draining payloads rather than every TCP object. Failures retain the slot
    // until reboot instead of admitting another listener against uncertain resources.
    drop(connection);
}

/// Why a listener's stream ended.
enum End {
    Retired,
    /// The ring overwrote frames the listener hadn't been sent: it read too slowly.
    FellBehind,
    /// Writing a frame exhausted its write budget: the listener stopped reading, or reads too
    /// little to matter.
    StoppedReading,
    /// Capture or the shared encoder stopped supplying fresh audio.
    CaptureStalled,
    /// A copied shared frame failed its codec-specific listener preparation.
    InvalidFrame(io::Error),
    /// The socket failed, usually because the listener closed it.
    Failed(io::Error),
}

impl End {
    fn from_write_error(e: io::Error) -> Self {
        // lwIP reports a send timeout as EAGAIN; the deadline running out is TimedOut.
        match e.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => End::StoppedReading,
            _ => End::Failed(e),
        }
    }
}

impl fmt::Display for End {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            End::Retired => f.write_str("format epoch retired"),
            End::FellBehind => f.write_str("fell a ring behind"),
            End::StoppedReading => f.write_str("stopped reading"),
            End::CaptureStalled => {
                write!(f, "no fresh audio for {} s", FRESH_AUDIO_TIMEOUT.as_secs())
            }
            End::Failed(e) => write!(f, "{e}"),
            End::InvalidFrame(e) => write!(f, "compressed frame invalid: {e}"),
        }
    }
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
fn stream_wav(sock: &TcpStream, ring: &SharedStreamRing) -> End {
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
        if let Err(e) = write_frame(&mut writer, &bytes, write_limit) {
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
fn write_frame(writer: &mut DeadlineWriter<'_>, bytes: &[u8], limit: Duration) -> io::Result<()> {
    let started = Instant::now();
    let result = writer.write_all_before(bytes, started + limit);
    if let Err(error) = &result {
        if matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ) {
            log::info!(
                "stream: frame write timed out elapsed_ms={} limit_ms={} bytes={}",
                started.elapsed().as_millis(),
                limit.as_millis(),
                bytes.len()
            );
        }
    }
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
    let mut chunk = vec![0i32; 2 * CHUNK_FRAMES];
    let mut block_number = 0u32;
    let mut yield_frames = 0usize;
    let mut metrics = EncoderMetrics::new(&listeners);
    let mut cursor = encoder_cursor(&raw, &listeners, &mut metrics);
    loop {
        if worker.encoder.frames_wanted() == worker.epoch.format.block_frames()
            && worker.service(&runtime)
        {
            block_number = 0;
            yield_frames = 0;
            // Per-codec throughput intervals cannot include frames from the retired format.
            metrics = EncoderMetrics::new(&listeners);
            // SAFETY: only this worker changes its own FreeRTOS priority at a block boundary.
            unsafe {
                sys::vTaskPrioritySet(
                    std::ptr::null_mut(),
                    u32::from(worker.epoch.format.priority()),
                )
            };
            cursor = encoder_cursor(&raw, &listeners, &mut metrics);
            log::info!("stream: active format {:?}", worker.epoch.format);
        }
        let frames = CHUNK_FRAMES.min(worker.encoder.frames_wanted());
        match read_some(
            &raw,
            &mut cursor,
            &mut chunk[..2 * frames],
            Some(&mut metrics.raw_read),
        ) {
            Ok(n) => worker.encoder.push(&chunk[..2 * n]),
            // Keep this one shared encoder alive during a pause. The next read starts a full
            // wait; a capture reset still yields Overrun and discards the partial block below.
            Err(WaitError::Stalled | WaitError::Read(ReadError::Retry)) => {
                metrics.log_if_due(&listeners);
                continue;
            }
            Err(WaitError::Read(ReadError::Overrun)) => {
                log::warn!(
                    "stream: encoder fell behind capture; disconnecting compressed listeners"
                );
                worker.epoch.ring.lock().expect("encoded ring lock").reset();
                if let Err(error) = worker.encoder.reset() {
                    log::error!("stream: encoder reset failed; output remains invalid ({error})");
                    return;
                }
                cursor = encoder_cursor(&raw, &listeners, &mut metrics);
                block_number = 0;
                metrics.resets += 1;
                metrics.log_if_due(&listeners);
                continue;
            }
        }
        if worker.encoder.frames_wanted() == 0 {
            // SAFETY: IDF initialized this monotonic timer before the encoder task starts.
            let encode_started_us = unsafe { sys::esp_timer_get_time() as u64 };
            let result = worker.encoder.encode_block(block_number, &mut worker.bytes);
            // The interval contains the codec call only, including any preemption during it.
            // SAFETY: the initialized monotonic timer remains available on this task.
            let encode_elapsed_us = unsafe { sys::esp_timer_get_time() as u64 } - encode_started_us;
            metrics.encode_max_us = metrics.encode_max_us.max(encode_elapsed_us);
            metrics.encode_wall_total_us += encode_elapsed_us;
            if let Err(error) = result {
                worker.epoch.ring.lock().expect("encoded ring lock").reset();
                log::error!("stream: encoding failed; disconnecting listeners ({error})");
                return;
            }
            // SAFETY: the initialized monotonic timer measures wall time, including preemption.
            let lock_started_us = unsafe { sys::esp_timer_get_time() as u64 };
            let mut output = worker.epoch.ring.lock().expect("encoded ring lock");
            // SAFETY: same initialized timer; acquisition is separate from the held copy below.
            let lock_acquired_us = unsafe { sys::esp_timer_get_time() as u64 };
            output
                .push(&worker.bytes)
                .expect("encoder frame fits encoded ring");
            // SAFETY: same initialized monotonic timer remains available while this lock is held.
            let push_finished_us = unsafe { sys::esp_timer_get_time() as u64 };
            drop(output);
            metrics.encoded_lock_wait_wall_total_us += lock_acquired_us - lock_started_us;
            metrics.encoded_push_held_wall_total_us += push_finished_us - lock_acquired_us;
            block_number = block_number.wrapping_add(1);
            metrics.frames += 1;
            metrics.log_if_due(&listeners);
            if worker.epoch.format == OutputFormat::Mp3 {
                yield_frames += 1;
                if yield_frames == 8 {
                    // Yield regularly even when capture already has more PCM.
                    // A one-tick RTOS delay yields; IDF sub-tick usleep can busy-wait.
                    // SAFETY: this code runs on the initialized encoder FreeRTOS task.
                    unsafe { sys::vTaskDelay(1) };
                    yield_frames = 0;
                }
            }
        }
    }
}

/// The shared encoder stays alive while capture exposes a busy header.
/// Initial and reset cursor snapshots share the same bounded wait policy.
fn encoder_cursor(
    raw: &SharedStreamRing,
    listeners: &AtomicUsize,
    metrics: &mut EncoderMetrics,
) -> Cursor {
    loop {
        if let Some(cursor) =
            cursor_before(raw, Instant::now() + FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP)
        {
            return cursor;
        }
        metrics.log_if_due(listeners);
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
        }
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

    fn log_if_due(&mut self, listeners: &AtomicUsize) {
        // SAFETY: IDF initializes the monotonic timer before the encoder task starts.
        let now = unsafe { sys::esp_timer_get_time() as u64 };
        if now - self.since_us < 10_000_000 {
            return;
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
        self.since_us = now;
        self.runtime_us = runtime;
        self.listeners = current;
        self.encode_max_us = 0;
        self.encode_wall_total_us = 0;
        self.encoded_lock_wait_wall_total_us = 0;
        self.encoded_push_held_wall_total_us = 0;
        self.raw_read = RawReadMetrics::default();
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

    fn log_departure(&self, peer: &str) {
        let (now, runtime) = EncoderMetrics::snapshot();
        log::info!("stream-listener: peer={} elapsed_us={} task_runtime_us={} runtime_valid={} frames_sent={} bytes_sent={} frame_write_wall_total_us={} frame_write_wall_max_us={} frame_read_wait_wall_total_us={} block_samples={}",
            peer, now - self.since_us, runtime.wrapping_sub(self.runtime_us),
            now <= u64::from(u32::MAX), self.frames_sent, self.bytes_sent,
            self.frame_write_wall_total_us, self.frame_write_wall_max_us,
            self.frame_read_wait_wall_total_us, self.block_frames);
        let d = &self.socket;
        log::info!("stream-sender: peer={} payload_elapsed_us={} send_calls={} accepted_bytes={} short_sends={} send_would_block={} send_interrupted={} send_errors={} send_wall_total_us={} send_wall_max_us={} send_over_10ms={}",
            peer, self.payload_elapsed_us, d.send_calls, d.accepted_bytes, d.short_sends,
            d.send_would_block, d.send_interrupted, d.send_errors, d.send_wall_total_us,
            d.send_wall_max_us, d.send_over_10ms);
        log::info!("stream-sender-poll: peer={} poll_calls={} poll_ready={} poll_timeouts={} poll_interrupted={} poll_errors={} poll_error_ready={} poll_wall_total_us={} poll_wall_max_us={} poll_over_10ms={} ready_to_send_wall_max_us={} ready_then_would_block={}",
            peer, d.poll_calls, d.poll_ready, d.poll_timeouts, d.poll_interrupted,
            d.poll_errors, d.poll_error_ready, d.poll_wall_total_us, d.poll_wall_max_us,
            d.poll_over_10ms, d.ready_to_send_wall_max_us, d.ready_then_would_block);
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
    metrics: &mut ListenerMetrics,
) -> End {
    let ring = &epoch.ring;
    let format = epoch.format;
    let write_limit = Duration::from_millis(format.write_limit_ms(CONFIG.shazam_enabled));
    let mut writer = DeadlineWriter::new(sock);
    let mut payload_started = None;
    let end = (|| {
        let mut header = format.format().response_head().as_bytes().to_vec();
        header.extend_from_slice(&codec::stream_header(format));
        let mut cursor = if let Some(preset) = preset {
            match buffered_cursor_before(
                ring,
                sock,
                preset.frames(format.block_frames()),
                Instant::now() + Duration::from_secs(preset.seconds()) + FRESH_AUDIO_TIMEOUT,
                FRESH_AUDIO_TIMEOUT,
                CAUGHT_UP_SLEEP,
            ) {
                Ok(cursor) => cursor,
                Err(error) => {
                    if error != BufferWaitError::Disconnected {
                        send_refusal(
                            sock,
                            "buffered listener",
                            "buffer startup",
                            Status::ServiceUnavailable,
                        );
                    }
                    return End::CaptureStalled;
                }
            }
        } else {
            let Some(ring) =
                lock_before(ring, Instant::now() + FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP)
            else {
                return End::CaptureStalled;
            };
            ring.cursor_now()
        };
        if epoch.retired() {
            return End::Retired;
        }
        if let Err(e) = write_frame(&mut writer, &header, write_limit) {
            return End::from_write_error(e);
        }
        writer.reset_diagnostics();
        payload_started = Some(Instant::now());
        let mut block_number = 0;
        loop {
            if epoch.retired() {
                return End::Retired;
            }
            let read_started = Instant::now();
            let read_result =
                read_locked_fresh(ring, FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP, |ring| {
                    if epoch.retired() {
                        return Err(esp_shazam_core::flac_ring::Overrun);
                    }
                    ring.read(&mut cursor, bytes)
                });
            metrics.frame_read_wait_wall_total_us += read_started.elapsed().as_micros() as u64;
            let n = match read_result {
                Ok(n) => n,
                Err(WaitError::Stalled) => return End::CaptureStalled,
                Err(WaitError::Read(_)) => return End::FellBehind,
            };
            let n = match codec::prepare_listener_frame(format, bytes, n, block_number) {
                Ok(n) => n,
                Err(error) => return End::InvalidFrame(error),
            };
            let write_started = Instant::now();
            let write_result = write_frame(&mut writer, &bytes[..n], write_limit);
            let write_elapsed_us = write_started.elapsed().as_micros() as u64;
            metrics.frame_write_wall_total_us += write_elapsed_us;
            metrics.frame_write_wall_max_us = metrics.frame_write_wall_max_us.max(write_elapsed_us);
            if let Err(e) = write_result {
                return End::from_write_error(e);
            }
            metrics.frames_sent += 1;
            metrics.bytes_sent += n as u64;
            // FLAC numbering advances after a whole write and rejects numbers at 2^31.
            // MP3 ignores this counter, so wrap cannot alter its frame bytes.
            block_number = block_number.wrapping_add(1);
        }
    })();
    metrics.socket = writer.diagnostics();
    metrics.payload_elapsed_us =
        payload_started.map_or(0, |started| started.elapsed().as_micros() as u64);
    end
}
