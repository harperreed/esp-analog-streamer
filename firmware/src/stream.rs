// ABOUTME: The LINE IN stream on port 8000: GET /stream.flac and GET /stream.wav send the stream ring as endless 48 kHz, 24-bit stereo.
// ABOUTME: Its threads run on core 1 and only copy out of the ring, so capture never waits on a socket; a listener a ring behind is dropped.

use std::fmt;
use std::io::{self, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::mdns::EspMdns;
use esp_shazam_core::stream_http::{self, DeadlineReader, DeadlineWriter, Format, Route, Status};
use esp_shazam_core::stream_ring::{Cursor, Overrun};
use esp_shazam_core::{flac, wav};

use crate::audio_in::{SharedStreamRing, CAPTURE_RATE_HZ};
use crate::threads::ThreadSpec;

/// The stream's port. The page and `/api/state` stay on port 80.
pub const PORT: u16 = 8000;
/// WAV and FLAC listeners share the slots, and a third listener of either gets 503. Each one takes a
/// thread, TCP buffers in internal RAM, and Wi-Fi: 2.3 Mbit/s for the WAV, less for FLAC.
const MAX_LISTENERS: usize = 2;
/// Frames per read from the ring: 5 ms. `read` holds the ring's lock while it copies, and the audio
/// thread's push waits for that lock. For the WAV they pack into 1,440 bytes, one full TCP segment;
/// a FLAC listener fills its 4,096-frame block with reads of at most this many.
const CHUNK_FRAMES: usize = 240;
// The FLAC encoder writes 48 kHz into every frame header.
const _: () = assert!(flac::SAMPLE_RATE_HZ == CAPTURE_RATE_HZ);
/// A listener that has caught up sleeps one FreeRTOS tick: ESP-IDF's usleep busy-waits anything shorter.
const CAUGHT_UP_SLEEP: Duration = Duration::from_millis(10);
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
const LISTENER_THREAD: ThreadSpec = ThreadSpec {
    name: c"stream-out",
    ..ACCEPT_THREAD
};

/// Listens on port 8000 on a thread of its own, and advertises the stream over mDNS as
/// "esp-shazam stream", next to the page's "esp-shazam".
pub fn start(ring: SharedStreamRing, mdns: &mut EspMdns) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", PORT))?;
    ACCEPT_THREAD.spawn(move || accept(listener, ring))?;
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

fn accept(listener: TcpListener, ring: SharedStreamRing) {
    let listeners = Arc::new(AtomicUsize::new(0));
    for conn in listener.incoming() {
        match conn {
            Ok(sock) => answer(sock, &ring, &listeners),
            Err(e) => {
                log::warn!("stream: accept failed: {e}");
                // lwIP drops a connection it has no socket for. Pause in case an error repeats at once.
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Reads one request and answers it: a listener thread for a stream, or a refusal.
fn answer(sock: TcpStream, ring: &SharedStreamRing, listeners: &Arc<AtomicUsize>) {
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
        Route::Stream(format) => match Slot::take(listeners) {
            Some(slot) => {
                log::info!(
                    "stream: {peer} joined {} ({} of {MAX_LISTENERS})",
                    format.path(),
                    slot.taken()
                );
                let ring = ring.clone();
                // On failure the closure is dropped with the socket and the slot: the client sees
                // the connection close.
                if let Err(e) =
                    LISTENER_THREAD.spawn(move || listen(sock, ring, slot, peer, format))
                {
                    log::warn!("stream: no thread for a listener: {e}");
                }
            }
            None => refuse(sock, &peer, &request_line, Status::ServiceUnavailable),
        },
        Route::Refuse(status) => refuse(sock, &peer, &request_line, status),
    }
}

/// Sends `status`'s complete response, then closes the connection.
fn refuse(mut sock: TcpStream, peer: &str, request_line: &str, status: Status) {
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

/// One of the `MAX_LISTENERS` listener places. Dropping it frees the place.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    /// Takes a place, if one is free.
    fn take(taken: &Arc<AtomicUsize>) -> Option<Self> {
        taken
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_LISTENERS).then_some(n + 1)
            })
            .ok()?;
        Some(Self(Arc::clone(taken)))
    }

    /// How many places are taken now, this one included.
    fn taken(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A listener's thread: streams `format` until the listener goes or falls behind, then frees its slot.
fn listen(sock: TcpStream, ring: SharedStreamRing, slot: Slot, peer: String, format: Format) {
    let started = Instant::now();
    let end = match format {
        Format::Wav => stream_wav(&sock, &ring),
        Format::Flac => stream_flac(&sock, &ring),
    };
    // Integer tenths: formatting a float to a precision puts a 1 KB buffer on this small stack.
    let tenths = started.elapsed().as_millis() / 100;
    log::info!(
        "stream: {peer} left after {}.{} s: {end}",
        tenths / 10,
        tenths % 10
    );
    // Close first, so the slot never frees while this connection still holds a socket. Closing sends
    // a FIN after any unsent audio. A reset would free lwIP's buffers sooner, but a stalled listener
    // never sees it: after zero-window probes lwIP's reset is one byte past what the peer expects
    // (RFC 5961 drops it), and a NAT such as Tailscale's subnet router closes its state on it, so
    // the client hangs. lwIP keeps a stalled peer's unsent audio, up to its 46,080-byte send buffer
    // (PSRAM: lwIP allocates there first), until the FIN gets through or 12 window probes (about
    // 7.6 minutes) run out. Such a closing connection no longer holds a slot, so `MAX_LISTENERS`
    // doesn't bound how many of them there are.
    drop(sock);
    drop(slot);
}

/// Why a listener's stream ended.
enum End {
    /// The ring overwrote frames the listener hadn't been sent: it read too slowly.
    FellBehind,
    /// Writing a frame took as long as the ring holds: the listener stopped reading, or reads too
    /// little to matter.
    StoppedReading,
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
            End::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// How long writing one frame may take: as long as the ring holds. A listener that takes longer has
/// stopped reading, and by then the ring has overwritten everything it wasn't sent. The limit holds
/// for the whole frame (`DeadlineWriter`), not for each send of it, so a trickle can't stretch it.
fn frame_write_limit(ring: &SharedStreamRing) -> Duration {
    let capacity = ring.lock().expect("stream ring lock").capacity() as u64;
    Duration::from_millis(1_000 * capacity / u64::from(CAPTURE_RATE_HZ))
}

/// Copies the frames after `cursor` into `out`, as many as are there and fit, and waits for some
/// when there are none. `out` holds at most `CHUNK_FRAMES` frames, so the lock is held briefly.
fn read_some(
    ring: &SharedStreamRing,
    cursor: &mut Cursor,
    out: &mut [i32],
) -> Result<usize, Overrun> {
    loop {
        // The lock is held for this statement only, never during encoding or socket work.
        let read = ring.lock().expect("stream ring lock").read(cursor, out);
        match read {
            Ok(0) => std::thread::sleep(CAUGHT_UP_SLEEP),
            other => return other,
        }
    }
}

/// Sends the response head and the WAV header, then frames from the ring as they arrive, until the
/// listener goes, stops reading, or falls a ring behind.
fn stream_wav(sock: &TcpStream, ring: &SharedStreamRing) -> End {
    let write_limit = frame_write_limit(ring);
    let mut writer = DeadlineWriter::new(sock);
    let mut frames = vec![0i32; 2 * CHUNK_FRAMES];
    let mut bytes = Vec::with_capacity(3 * frames.len());
    bytes.extend_from_slice(Format::Wav.response_head().as_bytes());
    bytes.extend_from_slice(&wav::endless_header_s24le(CAPTURE_RATE_HZ, 2));
    let mut cursor = ring.lock().expect("stream ring lock").cursor_now();
    loop {
        if let Err(e) = writer.write_all_before(&bytes, Instant::now() + write_limit) {
            return End::from_write_error(e);
        }
        let Ok(frames_read) = read_some(ring, &mut cursor, &mut frames) else {
            return End::FellBehind;
        };
        wav::pack_s24le(&frames[..2 * frames_read], &mut bytes);
    }
}

/// Sends the response head and the FLAC stream header, then one FLAC frame per 4,096 frames from the
/// ring, until the listener goes, stops reading, or falls a ring behind. Each listener encodes for
/// itself, on core 1.
fn stream_flac(sock: &TcpStream, ring: &SharedStreamRing) -> End {
    let write_limit = frame_write_limit(ring);
    let mut writer = DeadlineWriter::new(sock);
    // Ring reads land in a 1,920-byte chunk in internal RAM, and the encoder sorts each into its
    // block and sums its residuals while it's hot. The encoder's block (32 KB) and the frame (up to
    // 24.6 KB) are over malloc's 8,191-byte line, so they go to PSRAM; the chunk and the encoder's
    // partition sums (2.5 KB) stay in internal RAM. None of them sits on this thread's 4 KiB stack.
    let mut chunk = vec![0i32; flac::CHANNELS * CHUNK_FRAMES];
    let mut encoder = flac::Encoder::new();
    let mut bytes = Vec::with_capacity(flac::MAX_FRAME_BYTES);
    bytes.extend_from_slice(Format::Flac.response_head().as_bytes());
    bytes.extend_from_slice(&flac::stream_header());
    let mut cursor = ring.lock().expect("stream ring lock").cursor_now();
    let mut block_number = 0u32;
    loop {
        if let Err(e) = writer.write_all_before(&bytes, Instant::now() + write_limit) {
            return End::from_write_error(e);
        }
        while encoder.frames_wanted() > 0 {
            let frames = CHUNK_FRAMES.min(encoder.frames_wanted());
            match read_some(ring, &mut cursor, &mut chunk[..flac::CHANNELS * frames]) {
                Ok(n) => encoder.push(&chunk[..flac::CHANNELS * n]),
                Err(Overrun) => return End::FellBehind,
            }
        }
        encoder.encode_block(block_number, &mut bytes);
        block_number = block_number.wrapping_add(1);
    }
}
