// ABOUTME: Tests WAV freshness and listener-slot recovery using real loopback TCP sockets.
// ABOUTME: Suspends actual capture briefly; production logs and counters complete host assertions.

// Flash with scripts/flash --example wav-freshness, Shazam off and no external listeners.
// Require two HTTP 200 responses, exact WAV headers and advancing bytes before suspension;
// both EOFs 1.8–3.4 s after suspension; then two fresh HTTP 200 responses and advancing bytes.
// The host must also require two production no-fresh closures (not write timeouts), a restart
// request and a restart-counter increase. Byte progress is not decoded-audio validation.
// Always restore production with scripts/flash after this fixture.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys;
use esp_shazam_core::wav;

use esp_shazam_firmware::threads;

enum Event {
    Ready(usize),
    End(usize, u64, io::Result<()>),
}

struct Readers {
    stop: Arc<AtomicBool>,
    bytes: [Arc<AtomicU32>; 2],
    events: mpsc::Receiver<Event>,
    workers: Vec<JoinHandle<()>>,
}

impl Readers {
    fn start() -> io::Result<Self> {
        let (sender, events) = mpsc::channel();
        let mut readers = Self {
            stop: Arc::new(AtomicBool::new(false)),
            bytes: std::array::from_fn(|_| Arc::new(AtomicU32::new(0))),
            events,
            workers: Vec::with_capacity(2),
        };
        for id in 0..2 {
            let sender = sender.clone();
            let stop = readers.stop.clone();
            let bytes = readers.bytes[id].clone();
            readers
                .workers
                .push(
                    std::thread::Builder::new()
                        .stack_size(4 * 1024)
                        .spawn(move || {
                            let result = drain(id, &stop, &bytes, &sender);
                            let _ = sender.send(Event::End(id, millis(), result));
                        })?,
                );
        }
        Ok(readers)
    }

    fn stable(&self) -> anyhow::Result<()> {
        let mut ready = [false; 2];
        let deadline = Instant::now() + Duration::from_secs(4);
        while !ready.iter().all(|value| *value) {
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))?
            {
                Event::Ready(id) => ready[id] = true,
                Event::End(id, _, result) => {
                    anyhow::bail!("reader {id} ended before ready: {result:?}")
                }
            }
        }
        let before = self
            .bytes
            .each_ref()
            .map(|bytes| bytes.load(Ordering::Relaxed));
        std::thread::sleep(Duration::from_secs(2));
        for (id, before) in before.into_iter().enumerate() {
            let after = self.bytes[id].load(Ordering::Relaxed);
            anyhow::ensure!(
                after.wrapping_sub(before) >= 48_000,
                "reader {id} did not advance"
            );
            log::info!(
                "wav-freshness: marker=DATA id={id} bytes={after} ms={}",
                millis()
            );
        }
        anyhow::ensure!(
            self.events.try_recv().is_err(),
            "reader ended during baseline"
        );
        Ok(())
    }
}

impl Drop for Readers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn drain(
    id: usize,
    stop: &AtomicBool,
    bytes: &AtomicU32,
    sender: &mpsc::Sender<Event>,
) -> io::Result<()> {
    let address = SocketAddr::from(([127, 0, 0, 1], 8000));
    let mut socket = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    socket.set_write_timeout(Some(Duration::from_secs(1)))?;
    socket.write_all(b"GET /stream.wav HTTP/1.1\r\nHost: localhost\r\n\r\n")?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut head = [0u8; 512];
    let mut len = 0;
    loop {
        if len == head.len() {
            return Err(io::Error::other("HTTP head exceeds fixture bound"));
        }
        read_exact_before(&mut socket, &mut head[len..len + 1], deadline)?;
        len += 1;
        if head[..len].ends_with(b"\r\n\r\n") {
            break;
        }
    }
    if !head[..len].starts_with(b"HTTP/1.1 200 OK\r\n") {
        return Err(io::Error::other("expected HTTP 200"));
    }
    let mut header = [0; wav::HEADER_LEN];
    read_exact_before(&mut socket, &mut header, deadline)?;
    if header != wav::endless_header_s24le(48_000, 2) {
        return Err(io::Error::other("unexpected WAV header"));
    }
    let mut buffer = [0; 512];
    read_exact_before(&mut socket, &mut buffer[..6], deadline)?;
    bytes.store(6, Ordering::Relaxed);
    let _ = sender.send(Event::Ready(id));
    let deadline = Instant::now() + Duration::from_secs(15);
    while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
        match socket.read(&mut buffer) {
            Ok(0) => {
                log::info!(
                    "wav-freshness: marker=EOF id={id} bytes={} ms={}",
                    bytes.load(Ordering::Relaxed),
                    millis()
                );
                return Ok(());
            }
            Ok(n) => {
                bytes.fetch_add(n as u32, Ordering::Relaxed);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
    }
    socket.shutdown(Shutdown::Both)?;
    if stop.load(Ordering::Relaxed) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "fixture reader lifetime exceeded",
        ))
    }
}

fn read_exact_before(
    socket: &mut TcpStream,
    mut out: &mut [u8],
    deadline: Instant,
) -> io::Result<()> {
    while !out.is_empty() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "fixture header/data deadline",
            ));
        }
        match socket.read(out) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
            Ok(n) => {
                out = &mut out[n..];
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

struct Suspended(sys::TaskHandle_t);

impl Drop for Suspended {
    fn drop(&mut self) {
        // SAFETY: the checked production audio task is perpetual; restart never replaces it.
        unsafe { sys::vTaskResume(self.0) };
        log::info!("wav-freshness: marker=RESUME ms={}", millis());
    }
}

fn observe() -> anyhow::Result<()> {
    let readers = Readers::start()?;
    readers.stable()?;
    // SAFETY: static nul-terminated task name; production never deletes the audio owner task.
    let audio = unsafe { sys::xTaskGetHandle(c"audio".as_ptr()) };
    anyhow::ensure!(!audio.is_null(), "production audio task missing");
    log::info!("wav-freshness: marker=SUSPEND ms={}", millis());
    // SAFETY: suspend the checked perpetual capture task, not this observer. RAII resumes it.
    unsafe { sys::vTaskSuspend(audio) };
    let suspended = Suspended(audio);
    let at = millis();
    std::thread::sleep(Duration::from_millis(3_500));
    drop(suspended);
    let mut ended = [false; 2];
    for _ in 0..2 {
        match readers.events.recv_timeout(Duration::from_millis(200))? {
            Event::End(id, end, result) => {
                result?;
                let elapsed = end.saturating_sub(at);
                log::info!(
                    "wav-freshness: marker=EOF_ASSERT id={id} elapsed_ms={elapsed} ms={end}"
                );
                anyhow::ensure!(
                    (1_800..=3_400).contains(&elapsed),
                    "reader {id} EOF outside fresh budget"
                );
                anyhow::ensure!(!ended[id], "duplicate reader completion");
                ended[id] = true;
            }
            Event::Ready(_) => anyhow::bail!("unexpected readiness event"),
        }
    }
    drop(readers);
    std::thread::sleep(Duration::from_secs(1));
    log::info!("wav-freshness: marker=RECOVERY_CONNECT ms={}", millis());
    let recovered = Readers::start()?;
    recovered.stable()?;
    drop(recovered);
    log::info!("wav-freshness: marker=TRANSPORT_ASSERTIONS_PASSED ms={} (host must check restart/capture logs)", millis());
    Ok(())
}

fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        sys::CONFIG_LWIP_NETIF_LOOPBACK == 1,
        "lwIP loopback required"
    );
    threads::ThreadSpec {
        name: c"wav-freshness",
        core: Core::Core1,
        priority: 4,
        stack_bytes: 4 * 1024,
    }
    .spawn(|| {
        std::thread::sleep(Duration::from_secs(15));
        if let Err(error) = observe() {
            log::error!("wav-freshness: marker=FAIL ms={} error={error:#}", millis());
        }
    })?;
    esp_shazam_firmware::run()
}

fn millis() -> u64 {
    // SAFETY: ESP-IDF initializes the monotonic timer before main.
    unsafe { sys::esp_timer_get_time() as u64 / 1_000 }
}
