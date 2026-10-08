// ABOUTME: Firmware entry point: LINE IN capture, Wi-Fi, clock, mDNS, the recognizer (unless cfg turns Shazam off), the web server,
// ABOUTME: MQTT and the LINE IN stream on port 8000. The main thread ends up supervising Wi-Fi; everything else runs on its own thread.

mod audio_in;
#[cfg(feature = "capture-fixture")]
mod capture_fixture;
mod codec;
mod config;
mod i2s_capture;
mod mqtt_out;
mod net;
mod psram;
mod recognizer;
mod shazam_http;
mod socket_close;
mod sonos;
mod stream;
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
    // Raw frames feed WAV and the shared FLAC encoder; encoded frames hold FLAC's longer backlog.
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

    stream::start(stream_ring, &mut mdns)?;
    let sonos = sonos::start(store.clone(), nvs)?;
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
