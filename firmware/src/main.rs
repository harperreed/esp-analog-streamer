// ABOUTME: Firmware entry point: sets up LINE IN capture and logs level, peak and pitch once a second.
// ABOUTME: Wi-Fi, Shazam, the web page and MQTT join in later.

mod audio_in;
mod codec;

use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::units::FromValueType;
use esp_shazam_core::audio::{loudness_dbfs, zero_crossing_hz};

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let peripherals = Peripherals::take()?;

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
    codec::setup_line_in(&mut i2c, codec::PGA_0DB)?;
    let ring = audio_in::spawn_capture(rx)?;

    let mut second = Vec::with_capacity(audio_in::SAMPLE_RATE_HZ as usize);
    loop {
        std::thread::sleep(Duration::from_secs(1));
        ring.lock()
            .expect("audio ring lock")
            .copy_latest(audio_in::SAMPLE_RATE_HZ as usize, &mut second);
        let peak = second.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        log::info!(
            "audio: level {:.1} dBFS, peak {peak}, pitch {:.0} Hz",
            loudness_dbfs(&second),
            zero_crossing_hz(&second, audio_in::SAMPLE_RATE_HZ)
        );
    }
}
