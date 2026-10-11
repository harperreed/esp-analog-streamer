// ABOUTME: Firmware entry point: LINE IN capture, Wi-Fi, clock, mDNS, the recognizer (unless cfg turns Shazam off), the web server,
// ABOUTME: MQTT and the LINE IN stream on port 8000. The main thread ends up supervising Wi-Fi; everything else runs on its own thread.

mod audio_in;
mod capture_failure_telemetry;
#[cfg(feature = "capture-fixture")]
mod capture_fixture;
mod codec;
mod config;
mod i2s_capture;
mod listener_telemetry;
mod mqtt_out;
mod net;
mod psram;
mod recognizer;
mod shazam_http;
mod socket_close;
mod sonos;
mod stream;
mod stream_admission;
mod stream_codec;
mod stream_pipeline;
mod stream_runtime;
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
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // I2S first: the codec locks onto MCLK and LRCK when its state machines restart.
    let rx = audio_in::start_i2s(
        peripherals.i2s0,
        peripherals.pins.gpio27, // BCLK (v2 boards: gpio5)
        peripherals.pins.gpio35, // DIN
        peripherals.pins.gpio0,  // MCLK
        peripherals.pins.gpio25, // WS
    )?;
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio33, // SDA (v2 boards: gpio18)
        peripherals.pins.gpio32, // SCL (v2 boards: gpio23)
        &I2cConfig::new().baudrate(100.kHz().into()),
    )?;
    codec::setup_line_in(&mut i2c, CONFIG.pga_gain)?;
    let store: recognizer::SharedStore =
        Arc::new(Mutex::new(SongStore::new(20, CONFIG.shazam_enabled)));
    // ESP-IDF's first std mutex access allocates. Complete telemetry setup before either
    // producers or the metrics exporter can become the first queue accessor.
    listener_telemetry::EVENTS.initialize();
    encoder_telemetry::EVENTS.initialize();
    // Raw frames feed WAV and one shared encoder; encoded frames hold the longer backlog.
    let (ring, stream_ring) = audio_in::spawn_capture(rx, store.clone())?;
    #[cfg(feature = "capture-fixture")]
    capture_fixture::spawn(stream_ring.clone())?;

    let wifi = net::start_wifi(
        peripherals.modem,
        sysloop,
        nvs.clone(),
        CONFIG.wifi_ssid,
        CONFIG.wifi_pass,
    )?;
    let _sntp = net::start_sntp()?;
    let mut mdns = net::start_mdns(CONFIG.hostname)?;

    let network = wifi.network();
    stream::start(stream_ring, &mut mdns, network.clone())?;
    let sonos = sonos::start(store.clone(), nvs, network)?;
    let _server = web::start(store.clone(), sonos)?;
    let publisher = mqtt_out::start(CONFIG.mqtt_url, store.clone())?;
    // Capture only supplies a recognition ring when Shazam is enabled.
    if let Some(ring) = ring {
        recognizer::spawn(ring, store, config::policy_config(), move || {
            publisher.publish_now()
        })?;
    } else {
        log::info!("shazam: disabled by cfg");
    }

    net::supervise(wifi)
}

#[cfg(feature = "encoder-probe")]
mod encoder_probe;
mod encoder_telemetry;

#[cfg(feature = "listener-frame-state")]
mod listener_frame_state;
