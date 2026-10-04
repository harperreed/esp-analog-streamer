// ABOUTME: The LINE IN stream on port 8000: GET /stream.wav sends the stream ring as an endless 48 kHz, 24-bit stereo WAV.
// ABOUTME: Its threads run on core 1 and only copy out of the ring, so capture never waits on a socket; a listener 2 s behind is dropped.

use std::ffi::CStr;
use std::fmt;
use std::io::{self, BufReader, Write};
use std::mem::size_of;
use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::mdns::EspMdns;
use esp_idf_svc::sys;
use esp_shazam_core::stream_http::{self, Route, Status};
use esp_shazam_core::stream_ring::Overrun;
use esp_shazam_core::wav;

use crate::audio_in::{SharedStreamRing, CAPTURE_RATE_HZ};

/// The stream's port. The page and `/api/state` stay on port 80.
pub const PORT: u16 = 8000;
/// A third listener gets 503. Each one takes about 2.3 Mbit/s of Wi-Fi, a thread, and TCP buffers
/// in internal RAM.
const MAX_LISTENERS: usize = 2;
/// Frames per read from the ring: 5 ms, which packs into 1,440 bytes, one full TCP segment. `read`
/// holds the ring's lock while it copies, and the audio thread's push waits for that lock.
const CHUNK_FRAMES: usize = 240;
/// A listener that has caught up sleeps one FreeRTOS tick: ESP-IDF's usleep busy-waits anything shorter.
const CAUGHT_UP_SLEEP: Duration = Duration::from_millis(10);
/// A client gets this long to send its request head, which may be at most `MAX_HEAD_BYTES`.
const HEAD_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_HEAD_BYTES: u64 = 4096;
/// The server's threads run on core 1, which is idle while Shazam is off. Their priority is above
/// the recognizer's (5), so a fingerprint can't starve a listener, and below the audio thread (10),
/// lwIP (18) and Wi-Fi (23).
const PRIORITY: u8 = 7;
const ACCEPT_STACK: usize = 4 * 1024;
const LISTENER_STACK: usize = 4 * 1024;

/// Listens on port 8000 on a thread of its own, and advertises the stream over mDNS as
/// "esp-shazam stream", next to the page's "esp-shazam".
pub fn start(ring: SharedStreamRing, mdns: &mut EspMdns) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", PORT))?;
    spawn(c"stream", ACCEPT_STACK, move || accept(listener, ring))?;
    // The stream works without the advertisement, so a failure is only logged.
    if let Err(e) = mdns.add_service(
        Some("esp-shazam stream"),
        "_http",
        "_tcp",
        PORT,
        &[("path", stream_http::WAV_PATH)],
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

/// Reads one request and answers it: a listener thread for the WAV, or a refusal.
fn answer(sock: TcpStream, ring: &SharedStreamRing, listeners: &Arc<AtomicUsize>) {
    let peer = sock
        .peer_addr()
        .map_or_else(|_| "unknown peer".to_owned(), |addr| addr.to_string());
    let head = sock.set_read_timeout(Some(HEAD_TIMEOUT)).and_then(|()| {
        stream_http::read_request_line(BufReader::with_capacity(256, &sock), MAX_HEAD_BYTES)
    });
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
        Route::Wav => match Slot::take(listeners) {
            Some(slot) => {
                log::info!(
                    "stream: {peer} joined ({} of {MAX_LISTENERS})",
                    slot.taken()
                );
                let ring = ring.clone();
                // On failure the closure is dropped with the socket and the slot: the client sees
                // the connection close.
                if let Err(e) = spawn(c"stream-out", LISTENER_STACK, move || {
                    listen(sock, ring, slot, peer)
                }) {
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

/// A listener's thread: streams until the listener goes or falls behind, then frees its slot.
fn listen(mut sock: TcpStream, ring: SharedStreamRing, slot: Slot, peer: String) {
    let started = Instant::now();
    let end = stream_wav(&mut sock, &ring);
    // Integer tenths: formatting a float to a precision puts a 1 KB buffer on this small stack.
    let tenths = started.elapsed().as_millis() / 100;
    log::info!(
        "stream: {peer} left after {}.{} s: {end}",
        tenths / 10,
        tenths % 10
    );
    // Close first, so the slot never frees while this connection still holds a socket.
    drop(sock);
    drop(slot);
}

/// Why a listener's stream ended.
enum End {
    /// The ring overwrote frames the listener hadn't been sent: it read too slowly.
    FellBehind,
    /// A write waited as long as the ring holds: the listener stopped reading.
    StoppedReading,
    /// The socket failed, usually because the listener closed it.
    Failed(io::Error),
}

impl End {
    fn from_write_error(e: io::Error) -> Self {
        // lwIP reports a send timeout as EAGAIN.
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

/// Sends the response head and the WAV header, then frames from the ring as they arrive, until the
/// listener goes, stops reading, or falls a ring behind.
fn stream_wav(sock: &mut TcpStream, ring: &SharedStreamRing) -> End {
    let capacity = ring.lock().expect("stream ring lock").capacity() as u64;
    // A write that waits as long as the ring holds means the listener has stopped reading: by then
    // the ring has overwritten everything it wasn't sent.
    let ring_time = Duration::from_millis(1_000 * capacity / u64::from(CAPTURE_RATE_HZ));
    let options = sock
        .set_write_timeout(Some(ring_time))
        .and_then(|()| reset_on_close(sock));
    if let Err(e) = options {
        return End::Failed(e);
    }
    let mut frames = vec![0i32; 2 * CHUNK_FRAMES];
    let mut bytes = Vec::with_capacity(3 * frames.len());
    bytes.extend_from_slice(stream_http::WAV_RESPONSE_HEAD.as_bytes());
    bytes.extend_from_slice(&wav::endless_header_s24le(CAPTURE_RATE_HZ, 2));
    let mut cursor = ring.lock().expect("stream ring lock").cursor_now();
    loop {
        if let Err(e) = sock.write_all(&bytes) {
            return End::from_write_error(e);
        }
        let frames_read = loop {
            // The lock is held for this statement only, never during socket work.
            let read = ring
                .lock()
                .expect("stream ring lock")
                .read(&mut cursor, &mut frames);
            match read {
                Ok(0) => std::thread::sleep(CAUGHT_UP_SLEEP),
                Ok(n) => break n,
                Err(Overrun) => return End::FellBehind,
            }
        };
        wav::pack_s24le(&frames[..2 * frames_read], &mut bytes);
    }
}

/// Makes closing `sock` reset the connection while audio is still unsent (SO_LINGER of 0 s), so a
/// dropped listener's buffers are freed at once. Otherwise lwIP keeps a stalled listener's unsent
/// audio, up to 5.7 KB of internal RAM, while it probes the closed window for about 7.6 minutes.
/// Needs `CONFIG_LWIP_SO_LINGER` (sdkconfig.defaults).
fn reset_on_close(sock: &TcpStream) -> io::Result<()> {
    let linger = sys::linger {
        l_onoff: 1,
        l_linger: 0,
    };
    // SAFETY: the pointer and length describe `linger`, which outlives the call.
    let result = unsafe {
        sys::lwip_setsockopt(
            sock.as_raw_fd(),
            sys::SOL_SOCKET as _,
            sys::SO_LINGER as _,
            (&linger as *const sys::linger).cast(),
            size_of::<sys::linger>() as _,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Runs `f` on a thread of its own on core 1 at `PRIORITY`.
fn spawn(name: &'static CStr, stack_size: usize, f: impl FnOnce() + Send + 'static) -> Result<()> {
    ThreadSpawnConfiguration {
        name: Some(name),
        priority: PRIORITY,
        pin_to_core: Some(Core::Core1),
        ..Default::default()
    }
    .set()?;
    // Only Builder::stack_size sets a std thread's stack; ThreadSpawnConfiguration's is ignored.
    let spawned = std::thread::Builder::new().stack_size(stack_size).spawn(f);
    ThreadSpawnConfiguration::default().set()?;
    spawned?;
    Ok(())
}
