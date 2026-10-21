// ABOUTME: Firmware entry point: LINE IN capture, Wi-Fi, clock, mDNS, the recognizer (unless cfg turns Shazam off), the web server,
// ABOUTME: and the LINE IN stream on port 8000. The main thread ends up supervising Wi-Fi; everything else runs on its own thread.

mod audio_in;
mod capture_failure_telemetry;
#[cfg(feature = "capture-fixture")]
mod capture_fixture;
mod close_reaper;
mod codec;
mod config;
mod i2s_capture;
mod input_gain;
mod listener_telemetry;
mod net;
mod ota;
mod psram;
mod recognizer;
mod shazam_http;
mod socket_close;
mod sonos;
#[cfg(feature = "startup-capture-trace")]
mod startup_capture_trace;
#[cfg(feature = "sonos-fixture")]
pub use sonos::check_transport;
mod stream;
mod stream_admission;
mod stream_codec;
mod stream_pipeline;
mod stream_runtime;
mod stream_workers;
mod threads;
mod web;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::units::FromValueType;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_shazam_core::store::SongStore;

use config::CONFIG;

pub fn run() -> Result<()> {
    let result = run_inner();
    #[cfg(feature = "startup-capture-trace")]
    if result.is_err() {
        startup_capture_trace::export(&mut [0; 2]);
    }
    result
}

fn run_inner() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // I2S first: the codec locks onto MCLK and LRCK when its state machines restart.
    #[cfg(feature = "startup-capture-trace")]
    startup_capture_trace::phase(1, 0);
    let rx = audio_in::start_i2s(
        peripherals.i2s0,
        peripherals.pins.gpio27, // BCLK (v2 boards: gpio5)
        peripherals.pins.gpio35, // DIN
        peripherals.pins.gpio0,  // MCLK
        peripherals.pins.gpio25, // WS
    );
    #[cfg(feature = "startup-capture-trace")]
    startup_capture_trace::phase(2, if rx.is_ok() { 0 } else { -1 });
    let rx = rx?;
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio33, // SDA (v2 boards: gpio18)
        peripherals.pins.gpio32, // SCL (v2 boards: gpio23)
        &I2cConfig::new().baudrate(100.kHz().into()),
    )?;
    #[cfg(feature = "startup-capture-trace")]
    startup_capture_trace::phase(3, 0);
    let (gain_nvs, gain) = input_gain::load(nvs.clone(), CONFIG.pga_gain)?;
    let setup = codec::setup_line_in(&mut i2c, gain.register());
    #[cfg(feature = "startup-capture-trace")]
    startup_capture_trace::phase(4, if setup.is_ok() { 0 } else { -1 });
    setup?;
    let input = Arc::new(input_gain::Handle::new(i2c, gain_nvs, gain));
    #[cfg(feature = "ota-rollback-probe")]
    {
        log::error!("ota-rollback-probe: panicking on purpose so the bootloader must roll back");
        panic!("ota-rollback-probe");
    }
    let store: recognizer::SharedStore =
        Arc::new(Mutex::new(SongStore::new(20, CONFIG.shazam_enabled)));
    store
        .lock()
        .unwrap()
        .set_quiet_threshold(CONFIG.quiet_threshold_dbfs);
    // ESP-IDF's first std mutex access allocates. Complete telemetry setup before either
    // producers or the metrics exporter can become the first queue accessor.
    listener_telemetry::EVENTS.initialize();
    encoder_telemetry::EVENTS.initialize();
    device_metrics::CACHE.initialize();
    listener_observation::REGISTRY.initialize();
    // Raw frames feed WAV and one shared encoder; encoded frames hold the longer backlog.
    #[cfg(feature = "startup-capture-trace")]
    startup_capture_trace::phase(5, 0);
    let spawned = audio_in::spawn_capture(rx, store.clone());
    #[cfg(feature = "startup-capture-trace")]
    startup_capture_trace::phase(6, if spawned.is_ok() { 0 } else { -1 });
    let (ring, stream_ring) = spawned?;
    #[cfg(feature = "capture-fixture")]
    capture_fixture::spawn(stream_ring.clone())?;

    let wifi = net::start_wifi(
        peripherals.modem,
        sysloop,
        nvs.clone(),
        CONFIG.wifi_ssid,
        CONFIG.wifi_pass,
    )?;
    device_metrics_identity::initialize_after_wifi();
    let _sntp = net::start_sntp()?;
    let mut mdns = net::start_mdns(CONFIG.hostname)?;

    let network = wifi.network();
    ota::mark_valid_when_healthy(network.clone());
    stream::start(stream_ring, &mut mdns, network.clone())?;
    let sonos = sonos::start(store.clone(), nvs, network)?;
    let _server = web::start(store.clone(), sonos, input)?;
    // Capture only supplies a recognition ring when Shazam is enabled.
    if let Some(ring) = ring {
        recognizer::spawn(ring, store, config::policy_config())?;
    } else {
        log::info!("shazam: disabled by cfg");
    }

    net::supervise(wifi)
}

mod device_metrics;
mod device_metrics_identity;
#[cfg(feature = "encoder-probe")]
mod encoder_probe;
mod encoder_telemetry;
mod listener_observation;

#[cfg(feature = "listener-frame-state")]
mod listener_frame_state;
