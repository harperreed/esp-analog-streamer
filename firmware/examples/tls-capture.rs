// ABOUTME: Runs three real receive-only TLS/WebSocket cycles beside production capture and streams.
// ABOUTME: Requires sonos-fixture, recognition off, SONOS_FIXTURE_URI and an absolute SONOS_FIXTURE_CA.

// Start one/two FLAC listeners during BASELINE and retain UART/API evidence through recovery.
// RECOVERED means all transport cycles passed; capture continuity still needs unchanged fault
// counters/generation and exact decoded sample counts. Use only the local fixture, never a speaker.

#[cfg(feature = "sonos-fixture")]
use std::time::{Duration, Instant};

#[cfg(feature = "sonos-fixture")]
#[allow(dead_code)] // Only the build-time recognition flag is needed from this shared module.
#[path = "../src/config.rs"]
mod config;
#[cfg(feature = "sonos-fixture")]
#[allow(dead_code)] // Read the production clock without initializing another network stack.
#[path = "../src/net.rs"]
mod net;
#[cfg(feature = "sonos-fixture")]
mod recognizer {
    pub type SharedStore = std::sync::Arc<std::sync::Mutex<esp_shazam_core::store::SongStore>>;
}
#[cfg(feature = "sonos-fixture")]
#[allow(dead_code)] // Reuse the receive-only helper without launching a second controller.
#[path = "../src/sonos.rs"]
mod sonos;
#[cfg(feature = "sonos-fixture")]
#[path = "../src/threads.rs"]
mod threads;

#[cfg(feature = "sonos-fixture")]
const _: () = assert!(
    !config::CONFIG.shazam_enabled,
    "tls-capture requires shazam_enabled=false"
);

#[cfg(feature = "sonos-fixture")]
const CYCLES: u32 = 3;

#[cfg(feature = "sonos-fixture")]
fn experiment() -> anyhow::Result<()> {
    // Production owns Wi-Fi, NVS and SNTP; only wait for its already-running clock.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !net::clock_synced() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::ensure!(net::clock_synced(), "Fixture TLS requires a synced clock");

    let mut failures = 0;
    for cycle in 1..=CYCLES {
        marker("CONNECT_BEGIN", cycle);
        let started = Instant::now();
        let result = sonos::check_transport(
            env!("SONOS_FIXTURE_URI"),
            concat!(include_str!(env!("SONOS_FIXTURE_CA")), "\0").as_bytes(),
            Some(Duration::from_secs(2)),
        );
        match result {
            Ok(()) => log::info!(
                "tls-capture: marker=CYCLE_END cycle={cycle} ms={} elapsed_ms={} ok=true",
                millis(),
                started.elapsed().as_millis(),
            ),
            Err(error) => {
                failures += 1;
                log::error!(
                    "tls-capture: marker=CYCLE_END cycle={cycle} ms={} elapsed_ms={} ok=false error={error}",
                    millis(),
                    started.elapsed().as_millis(),
                );
            }
        }
        if cycle < CYCLES {
            std::thread::sleep(Duration::from_secs(10));
        }
    }
    // Preserve a capture-only recovery window even when a transport cycle failed.
    std::thread::sleep(Duration::from_secs(20));
    anyhow::ensure!(failures == 0, "{failures} of {CYCLES} TLS cycles failed");
    marker("RECOVERED", CYCLES);
    Ok(())
}

#[cfg(feature = "sonos-fixture")]
fn marker(name: &str, cycle: u32) {
    log::info!(
        "tls-capture: marker={name} cycle={cycle} ms={} cycles={CYCLES}",
        millis(),
    );
}

#[cfg(feature = "sonos-fixture")]
fn millis() -> u64 {
    // SAFETY: ESP-IDF starts this monotonic timer before entering main.
    unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 / 1_000 }
}

#[cfg(feature = "sonos-fixture")]
fn main() -> anyhow::Result<()> {
    threads::ThreadSpec {
        name: c"tls-capture",
        core: esp_idf_svc::hal::cpu::Core::Core0,
        priority: 2,
        stack_bytes: 16 * 1024,
    }
    .spawn(|| {
        // Give production startup time to initialize logging, capture, Wi-Fi and SNTP.
        std::thread::sleep(Duration::from_secs(15));
        marker("BASELINE", 0);
        std::thread::sleep(Duration::from_secs(25));
        if let Err(error) = experiment() {
            log::error!("tls-capture: marker=FAILED ms={} error={error}", millis());
        }
    })?;
    esp_shazam_firmware::run()
}

#[cfg(not(feature = "sonos-fixture"))]
fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    anyhow::bail!("tls-capture requires --features sonos-fixture and a local TLS fixture")
}
