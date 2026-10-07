// ABOUTME: Compile-only by default; sonos-fixture enables the real TLS receive-only diagnostic.
// ABOUTME: Set SONOS_FIXTURE_URI and absolute SONOS_FIXTURE_CA at build time; never run on a speaker.

#[path = "../src/threads.rs"]
mod threads;

mod recognizer {
    pub type SharedStore = std::sync::Arc<std::sync::Mutex<esp_shazam_core::store::SongStore>>;
}
#[path = "../src/sonos.rs"]
mod sonos;

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    // Keep every public entry point reachable for target compile/link verification only.
    let _ = sonos::start;
    let _ = sonos::Handle::snapshot;
    let _ = sonos::Handle::queue_snapshot;
    let _ = sonos::Handle::submit;
    let _ = [
        sonos::Command::Discover,
        sonos::Command::Select(String::new()),
        sonos::Command::Play,
        sonos::Command::Stop,
        sonos::Command::SetAutoplay(false),
    ];
    #[cfg(feature = "sonos-fixture")]
    fixture()?;
    Ok(())
}

#[cfg(feature = "sonos-fixture")]
#[path = "../src/config.rs"]
mod config;
#[cfg(feature = "sonos-fixture")]
#[path = "../src/net.rs"]
mod net;

#[cfg(feature = "sonos-fixture")]
fn fixture() -> anyhow::Result<()> {
    use esp_idf_svc::{
        eventloop::EspSystemEventLoop, hal::peripherals::Peripherals, nvs::EspDefaultNvsPartition,
    };
    use std::time::{Duration, Instant};
    esp_idf_svc::log::EspLogger::initialize_default();
    let peripherals = Peripherals::take()?;
    let wifi = net::start_wifi(
        peripherals.modem,
        EspSystemEventLoop::take()?,
        EspDefaultNvsPartition::take()?,
        config::CONFIG.wifi_ssid,
        config::CONFIG.wifi_pass,
    )?;
    let _clock = net::start_sntp()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !net::clock_synced() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::ensure!(net::clock_synced(), "Fixture TLS requires a synced clock");
    // Keep all reused modules' public functions reachable without running mDNS/recognition.
    let _ = (net::start_mdns, net::supervise, config::policy_config);
    let cancel_after = option_env!("SONOS_FIXTURE_CANCEL_MS")
        .map(str::parse::<u64>)
        .transpose()?
        .map(Duration::from_millis);
    anyhow::ensure!(
        cancel_after.is_none_or(|after| !after.is_zero() && after <= Duration::from_secs(10)),
        "SONOS_FIXTURE_CANCEL_MS must be 1..10000 when set"
    );
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    threads::ThreadSpec {
        name: c"sonos-fixture",
        core: esp_idf_svc::hal::cpu::Core::Core0,
        priority: 2,
        stack_bytes: 16 * 1024,
    }
    .spawn(move || {
        let result = sonos::check_transport(
            env!("SONOS_FIXTURE_URI"),
            concat!(include_str!(env!("SONOS_FIXTURE_CA")), "\0").as_bytes(),
            cancel_after,
        );
        let _ = sender.send(result);
    })?;
    receiver.recv_timeout(Duration::from_secs(30))??;
    log::info!("sonos-fixture: finished; restore normal firmware before production use");
    // Retain the Wi-Fi driver while the parent collects UART evidence.
    while wifi.is_connected().unwrap_or(false) {
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}
