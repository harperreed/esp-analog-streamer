// ABOUTME: The LINE IN stream on port 8000: GET /stream.flac and GET /stream.wav send endless 48 kHz stereo: 16-bit FLAC and 24-bit WAV.
// ABOUTME: One core 1 encoder shares FLAC frames; listeners copy out before socket work and drop if they fall behind.

use std::fmt;
use std::io::{self, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::mdns::EspMdns;
use esp_idf_svc::sys;
use esp_shazam_core::flac_ring::FlacRing;
use esp_shazam_core::stream_http::{self, DeadlineReader, DeadlineWriter, Format, Route, Status};
use esp_shazam_core::stream_ring::{Cursor, Overrun};
use esp_shazam_core::stream_wait::{lock_before, read_locked_fresh, Slot, WaitError};
use esp_shazam_core::{flac, wav};

use crate::audio_in::{SharedStreamRing, CAPTURE_RATE_HZ};
use crate::config::CONFIG;
use crate::psram::PsramBytes;
use crate::socket_close;
use crate::threads::ThreadSpec;

type SharedFlacRing = Arc<Mutex<FlacRing>>;
static RAW_READ_MAX_US: AtomicU32 = AtomicU32::new(0);

/// Interval maximum of the raw read closure, excluding lock acquisition and its unlock tail.
pub fn take_raw_read_max_us() -> u32 {
    RAW_READ_MAX_US.swap(0, Ordering::Relaxed)
}

/// A FLAC listener requires byte-addressable PSRAM for its complete encoded frame.
/// Small headers and the WAV's small buffers retain the normal allocator's preferred placement.
enum ListenerBuffer {
    Wav,
    Flac(PsramBytes),
}

impl ListenerBuffer {
    fn try_new(format: Format) -> io::Result<Self> {
        match format {
            Format::Wav => Ok(Self::Wav),
            Format::Flac => PsramBytes::try_new(flac::MAX_FRAME_BYTES).map(Self::Flac),
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

/// Off: 94 slots hold 8.02 s; on: 24 slots hold 2.048 s. The enabled profile
/// trades 576 KB of raw WAV slack for 590,280 bytes of encoded storage (including lengths).
/// Capture remains raw 48 kHz stereo; only FLAC listeners use this longer backlog.
const FLAC_RING_FRAMES: usize = if CONFIG.shazam_enabled {
    24
} else {
    (8 * CAPTURE_RATE_HZ as usize).div_ceil(flac::BLOCK_FRAMES)
};

/// The stream's port. The page and `/api/state` stay on port 80.
pub const PORT: u16 = 8000;
/// WAV and FLAC listeners share the slots, and a third listener of either gets 503. Each one takes a
/// thread, TCP buffers that prefer PSRAM, and Wi-Fi: 2.3 Mbit/s for the WAV, less for FLAC.
const MAX_LISTENERS: usize = 2;
/// Frames per read from the ring: 5 ms. `read` holds the ring's lock while it copies; capture stages
/// audio if that lock is busy. For the WAV they pack into 1,440 bytes, one full TCP segment;
/// the shared encoder fills its 4,096-frame block with reads of at most this many.
const CHUNK_FRAMES: usize = 240;
// The FLAC encoder writes 48 kHz into every frame header.
const _: () = assert!(flac::SAMPLE_RATE_HZ == CAPTURE_RATE_HZ);
/// A listener that has caught up sleeps one FreeRTOS tick: ESP-IDF's usleep busy-waits anything shorter.
const CAUGHT_UP_SLEEP: Duration = Duration::from_millis(10);
/// A caught-up listener waits at most 2 s for real audio, plus one tick and scheduling delay.
/// Quiet audio still produces samples/FLAC frames. Capture stalls release both listener slots
/// independently of the capture supervisor; the shared encoder waits again after this timeout.
const FRESH_AUDIO_TIMEOUT: Duration = Duration::from_secs(2);
/// A client gets this long to send its whole request head, which may be at most `MAX_HEAD_BYTES`,
/// and to take a refusal. The accept thread answers one connection at a time, so this is how long
/// one client can hold up the next.
const HEAD_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_HEAD_BYTES: u64 = 4096;
/// The server's threads run on core 1, which is idle while Shazam is off. Their priority, 7, is
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
    let encoded = FlacRing::try_new(FLAC_RING_FRAMES, flac::MAX_FRAME_BYTES)
        .map_err(|e| anyhow::anyhow!("stream: FLAC ring allocation failed: {e:?}"))?;
    let encoded = Arc::new(Mutex::new(encoded));
    let listeners = Arc::new(AtomicUsize::new(0));
    let encoder_listeners = listeners.clone();
    let (encoder_raw, encoder_output) = (ring.clone(), encoded.clone());
    ENCODER_THREAD.spawn(move || encode(encoder_raw, encoder_output, encoder_listeners))?;
    ACCEPT_THREAD.spawn(move || accept(listener, ring, encoded, listeners))?;
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
    encoded: SharedFlacRing,
    listeners: Arc<AtomicUsize>,
) {
    for conn in listener.incoming() {
        match conn {
            Ok(sock) => answer(sock, &ring, &encoded, &listeners),
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
    encoded: &SharedFlacRing,
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
    match stream_http::route(&request_line) {
        Route::Stream(format) => {
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
                    let encoded = encoded.clone();
                    // The owned connection's drop observes close even if no thread can be created.
                    if let Err(e) =
                        LISTENER_THREAD.spawn(move || listen(connection, ring, encoded, buffer))
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
    encoded: SharedFlacRing,
    buffer: ListenerBuffer,
) {
    let sock = connection.socket();
    let peer = &connection.peer;
    let started = Instant::now();
    let end = match buffer {
        ListenerBuffer::Wav => stream_wav(sock, &ring),
        ListenerBuffer::Flac(mut bytes) => stream_flac(sock, &encoded, bytes.as_mut_slice()),
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
    /// The ring overwrote frames the listener hadn't been sent: it read too slowly.
    FellBehind,
    /// Writing a frame took as long as the ring holds: the listener stopped reading, or reads too
    /// little to matter.
    StoppedReading,
    /// Capture or the shared encoder stopped supplying fresh audio.
    CaptureStalled,
    /// A copied shared frame could not be numbered for this listener.
    InvalidFlac(flac::RenumberError),
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
            End::FellBehind => f.write_str("fell a ring behind"),
            End::StoppedReading => f.write_str("stopped reading"),
            End::CaptureStalled => {
                write!(f, "no fresh audio for {} s", FRESH_AUDIO_TIMEOUT.as_secs())
            }
            End::Failed(e) => write!(f, "{e}"),
            End::InvalidFlac(e) => write!(f, "FLAC numbering failed: {e:?}"),
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

/// Copies the frames after `cursor` into `out`, as many as are there and fit, and waits for some
/// for at most `FRESH_AUDIO_TIMEOUT` when there are none. `out` holds at most `CHUNK_FRAMES`
/// frames, so the lock is held briefly.
fn read_some(
    ring: &SharedStreamRing,
    cursor: &mut Cursor,
    out: &mut [i32],
) -> Result<usize, WaitError<Overrun>> {
    read_locked_fresh(ring, FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP, |ring| {
        // Wall time includes preemption while copying; excludes encoding/socket work and
        // the tiny measurement/return/unlock tail after the elapsed sample below.
        let started = Instant::now();
        let result = ring.read(cursor, out);
        RAW_READ_MAX_US.fetch_max(
            started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32,
            Ordering::Relaxed,
        );
        result
    })
}

/// Sends the response head and the WAV header, then frames from the ring as they arrive, until the
/// listener goes, stops reading, or falls a ring behind.
fn stream_wav(sock: &TcpStream, ring: &SharedStreamRing) -> End {
    let (write_limit, mut cursor) = {
        let Some(ring) = lock_before(ring, Instant::now() + FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP)
        else {
            return End::CaptureStalled;
        };
        (frame_write_limit(ring.capacity()), ring.cursor_now())
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
        let frames_read = match read_some(ring, &mut cursor, &mut frames) {
            Ok(n) => n,
            Err(WaitError::Stalled) => return End::CaptureStalled,
            Err(WaitError::Read(Overrun)) => return End::FellBehind,
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

/// One encoder converts raw 24-bit capture to 16-bit FLAC once for every listener.
/// A raw overrun discards the partial block and invalidates old encoded cursors: existing
/// listeners disconnect rather than hearing discontinuous audio presented as a continuous stream.
fn encode(raw: SharedStreamRing, encoded: SharedFlacRing, listeners: Arc<AtomicUsize>) {
    let mut chunk = vec![0i32; flac::CHANNELS * CHUNK_FRAMES];
    let mut encoder = flac::Encoder::new();
    let mut bytes = Vec::with_capacity(flac::MAX_FRAME_BYTES);
    let mut block_number = 0u32;
    let mut metrics = EncoderMetrics::new(&listeners);
    let mut cursor = encoder_cursor(&raw, &listeners, &mut metrics);
    loop {
        let frames = CHUNK_FRAMES.min(encoder.frames_wanted());
        match read_some(&raw, &mut cursor, &mut chunk[..flac::CHANNELS * frames]) {
            Ok(n) => encoder.push(&chunk[..flac::CHANNELS * n]),
            // Keep this one shared encoder alive during a pause. The next read starts a full
            // wait; a capture reset still yields Overrun and discards the partial block below.
            Err(WaitError::Stalled) => {
                metrics.log_if_due(&listeners);
                continue;
            }
            Err(WaitError::Read(Overrun)) => {
                log::warn!("stream: encoder fell behind capture; disconnecting FLAC listeners");
                encoded.lock().expect("FLAC ring lock").reset();
                encoder.reset();
                cursor = encoder_cursor(&raw, &listeners, &mut metrics);
                block_number = 0;
                metrics.resets += 1;
                metrics.log_if_due(&listeners);
                continue;
            }
        }
        if encoder.frames_wanted() == 0 {
            encoder.encode_block(block_number, &mut bytes);
            encoded
                .lock()
                .expect("FLAC ring lock")
                .push(&bytes)
                .expect("encoder frame fits FLAC ring");
            block_number = block_number.wrapping_add(1);
            metrics.frames += 1;
            metrics.log_if_due(&listeners);
        }
    }
}

/// The shared encoder stays alive if capture is suspended while owning the raw ring.
/// Each failed acquisition waits a full budget; initial and reset cursors use the same path.
fn encoder_cursor(
    raw: &SharedStreamRing,
    listeners: &AtomicUsize,
    metrics: &mut EncoderMetrics,
) -> Cursor {
    loop {
        if let Some(ring) = lock_before(raw, Instant::now() + FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP)
        {
            return ring.cursor_now();
        }
        metrics.log_if_due(listeners);
    }
}

/// Task runtime accounting excludes its current scheduling slice and includes interrupt/logging
/// overhead charged to this task. Compare stable listener windows, not encode-call wall latency.
struct EncoderMetrics {
    since_us: u64,
    runtime_us: u32,
    listeners: usize,
    frames: u64,
    resets: u64,
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
        // U32 runtime stats lose accounting at the timer wrap (~71.6 min). Mark later
        // windows invalid rather than presenting wrapping arithmetic as accurate CPU use.
        log::info!("stream-encode: elapsed_us={} task_runtime_us={} runtime_valid={} encoded_frames={} resets={} listeners_start={} listeners_end={} (slots include WAV and closing listeners)",
            now - self.since_us, runtime.wrapping_sub(self.runtime_us), now <= u64::from(u32::MAX),
            self.frames, self.resets, self.listeners, current);
        self.since_us = now;
        self.runtime_us = runtime;
        self.listeners = current;
    }
}

/// A fresh header precedes shared frames whose copied headers count from zero for this listener.
/// The bounded output buffer holds one complete frame, so every socket write shares that frame's
/// single deadline. The ring's mutex is released before any socket call.
fn stream_flac(sock: &TcpStream, ring: &SharedFlacRing, bytes: &mut [u8]) -> End {
    let write_limit = Duration::from_millis(
        1_000 * (FLAC_RING_FRAMES * flac::BLOCK_FRAMES) as u64 / u64::from(CAPTURE_RATE_HZ),
    );
    let mut writer = DeadlineWriter::new(sock);
    let mut header = Format::Flac.response_head().as_bytes().to_vec();
    header.extend_from_slice(&flac::stream_header());
    let mut cursor = {
        let Some(ring) = lock_before(ring, Instant::now() + FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP)
        else {
            return End::CaptureStalled;
        };
        ring.cursor_now()
    };
    if let Err(e) = write_frame(&mut writer, &header, write_limit) {
        return End::from_write_error(e);
    }
    let mut block_number = 0;
    loop {
        let n = match read_locked_fresh(ring, FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP, |ring| {
            ring.read(&mut cursor, bytes)
        }) {
            Ok(n) => n,
            Err(WaitError::Stalled) => return End::CaptureStalled,
            Err(WaitError::Read(_)) => return End::FellBehind,
        };
        let n = match flac::renumber_frame(bytes, n, block_number) {
            Ok(n) => n,
            Err(error) => return End::InvalidFlac(error),
        };
        if let Err(e) = write_frame(&mut writer, &bytes[..n], write_limit) {
            return End::from_write_error(e);
        }
        // Advance only after the whole frame was written. At 2^31 the helper rejects the
        // next frame, ending through normal bounded connection cleanup rather than wrapping.
        block_number += 1;
    }
}
