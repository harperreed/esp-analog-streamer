// ABOUTME: Sends two finite deterministic TCP payloads without capture, encoding or application traffic.
// ABOUTME: Isolates Wi-Fi transport with production socket deadlines; requires a real host byte check.

// Throwaway diagnostic: scripts/flash --example wifi-throughput, then
// scripts/wifi-throughput-check HOST --out NEW_REPORT.json. Restore production afterward.
// Verify PSRAM initialized and static TX16/cache32, send46080/MSS1440 on UART first.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys;
use esp_shazam_core::stream_http::DeadlineWriter;
use esp_shazam_firmware::{config, net, psram, socket_close, threads};

const PORT: u16 = 8675;
// Current quiet-input production delivery was approximately 133,480 B/s per peer.
// Offer 135,168 B/s for 117 s; this is a comparable load, not worst-case audio.
const CHUNK: usize = 11_264;
const CHUNK_COUNT: usize = 1_404;
const TARGET: usize = CHUNK * CHUNK_COUNT;
const CHUNKS_PER_SECOND: u64 = 12;
const WRITE_LIMIT: Duration = Duration::from_millis(8021);
const TOTAL_LIMIT: Duration = Duration::from_secs(120);
const ADMISSION_LIMIT: Duration = Duration::from_secs(20);

fn main() -> anyhow::Result<()> {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    anyhow::ensure!(!config::CONFIG.shazam_enabled, "R26 requires Shazam off");
    let peripherals = Peripherals::take()?;
    let wifi = net::start_wifi(
        peripherals.modem,
        EspSystemEventLoop::take()?,
        EspDefaultNvsPartition::take()?,
        config::CONFIG.wifi_ssid,
        config::CONFIG.wifi_pass,
    )?;
    let listener = TcpListener::bind(("0.0.0.0", PORT))?;
    listener.set_nonblocking(true)?;
    heap_snapshot("before_admission");
    log::info!("wifi-throughput: READY port={PORT} peers=2 bytes={TARGET} admission_s=20");
    let admission = Instant::now() + ADMISSION_LIMIT;
    let mut peers = Vec::with_capacity(2);
    while peers.len() < 2 && Instant::now() < admission {
        match listener.accept() {
            Ok((sock, _)) => {
                if socket_close::configure(&sock).is_err() {
                    // No verified linger policy: retain this owner through immediate recovery.
                    let _socket = sock;
                    restart();
                }
                match psram::PsramBytes::try_new(CHUNK) {
                    Ok(buffer) => peers.push((sock, buffer)),
                    Err(error) => {
                        checked_close(sock);
                        for (sock, _) in peers.drain(..) {
                            checked_close(sock);
                        }
                        return Err(error.into());
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                for (sock, _) in peers.drain(..) {
                    checked_close(sock);
                }
                return Err(error.into());
            }
        }
    }
    drop(listener);
    if peers.len() != 2 {
        for (sock, _) in peers {
            checked_close(sock);
        }
        anyhow::bail!("two peers did not arrive within admission deadline");
    }
    let start = Instant::now() + Duration::from_millis(100);
    for (peer, (sock, buffer)) in peers.into_iter().enumerate() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let spawned = threads::ThreadSpec {
            name: c"wifi-throughput",
            core: Core::Core1,
            priority: 7,
            stack_bytes: 4 * 1024,
        }
        .spawn(move || {
            if let Ok((sock, buffer)) = receiver.recv_timeout(ADMISSION_LIMIT) {
                transfer(peer, sock, buffer, start);
            }
        });
        if spawned.is_err() {
            checked_close(sock);
            // Another worker may already own a socket; reboot ends this incomplete experiment.
            restart();
        }
        if let Err(error) = sender.send((sock, buffer)) {
            checked_close(error.0 .0);
            restart();
        }
    }
    net::supervise(wifi)
}

fn transfer(peer: usize, sock: TcpStream, mut buffer: psram::PsramBytes, start: Instant) {
    std::thread::sleep(start.saturating_duration_since(Instant::now()));
    let deadline = start + TOTAL_LIMIT;
    let mut count = 0usize;
    let result = (|| -> io::Result<()> {
        let mut writer = DeadlineWriter::new(&sock);
        let header = format!("ESPTHR1 {TARGET}\n");
        writer.write_all_before(
            header.as_bytes(),
            deadline.min(Instant::now() + WRITE_LIMIT),
        )?;
        for chunk_number in 0..CHUNK_COUNT {
            let due = start
                + Duration::from_nanos(chunk_number as u64 * 1_000_000_000 / CHUNKS_PER_SECOND);
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
            let len = CHUNK;
            let bytes = &mut buffer.as_mut_slice()[..len];
            for (index, byte) in bytes.iter_mut().enumerate() {
                let offset = count + index;
                *byte = (offset as u8) ^ ((offset >> 8) as u8) ^ ((offset >> 16) as u8);
            }
            writer.write_all_before(bytes, deadline.min(due + WRITE_LIMIT))?;
            count += len;
        }
        Ok(())
    })();
    let elapsed = start.elapsed().as_millis();
    checked_close(sock);
    heap_snapshot("worker_completion_concurrent_buffer_owned");
    log::info!("wifi-throughput: peer={peer} completed_bytes={count} elapsed_ms={elapsed} result={result:?}");
}

fn heap_snapshot(phase: &str) {
    // SAFETY: public heap queries accept capability flags and return allocator snapshots.
    let (internal_free, internal_largest, psram_free, psram_largest) = unsafe {
        let internal = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
        let external = sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT;
        (
            sys::heap_caps_get_free_size(internal),
            sys::heap_caps_get_largest_free_block(internal),
            sys::heap_caps_get_free_size(external),
            sys::heap_caps_get_largest_free_block(external),
        )
    };
    log::info!("wifi-throughput: heap phase={phase} internal_free={internal_free} internal_largest={internal_largest} psram_free={psram_free} psram_largest={psram_largest}");
}

fn checked_close(sock: TcpStream) {
    match socket_close::close(sock) {
        Ok(report) => log::info!(
            "wifi-throughput: closed elapsed_ms={}",
            report.elapsed.as_millis()
        ),
        Err(failure) => {
            // Keep ownership alive across immediate recovery; never log or retry an uncertain FD.
            let _failure = failure;
            restart();
        }
    }
}

fn restart() -> ! {
    // SAFETY: public IDF reboot API takes no pointers and never returns.
    unsafe { sys::esp_restart() }
}
