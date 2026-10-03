// ABOUTME: Bring-up firmware: logs PSRAM and internal RAM, probes for the codec, and times one fingerprint.
// ABOUTME: Its serial output settles the "(verify)" items in the spec's Hardware section.

mod board;

use std::time::Instant;

use anyhow::Result;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::sys;
use esp_shazam_core::fingerprinting::algorithm::SignatureGenerator;
use esp_shazam_core::signal::chord_and_chirp;

/// The fingerprint needs about 2.1 MB; leave room for the audio ring and buffers.
const PSRAM_NEEDED_BYTES: usize = 3 * 1024 * 1024;

fn main() -> Result<()> {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let peripherals = Peripherals::take()?;

    let psram = log_memory("boot");
    let revision = board::probe(
        peripherals.i2c0,
        peripherals.pins.gpio33,
        peripherals.pins.gpio32,
        peripherals.i2c1,
        peripherals.pins.gpio18,
        peripherals.pins.gpio23,
    )?;
    log::info!("bring-up: codec revision {revision:?}");

    if psram < PSRAM_NEEDED_BYTES {
        log::error!(
            "bring-up: only {} KB of PSRAM; skipping the fingerprint timing",
            psram / 1024
        );
        return Ok(());
    }
    let pcm = chord_and_chirp(12);
    let started = Instant::now();
    let signature = SignatureGenerator::make_signature_from_buffer(&pcm);
    let took_ms = started.elapsed().as_millis();
    let bytes = signature.encode_to_binary().map(|b| b.len()).unwrap_or(0);
    log::info!("bring-up: fingerprint of 12 s took {took_ms} ms ({bytes} bytes)");
    log_memory("after fingerprint");
    Ok(())
}

/// Logs memory and returns the PSRAM size in bytes (0 when PSRAM didn't initialize).
fn log_memory(when: &str) -> usize {
    // SAFETY: read-only queries of ESP-IDF's PSRAM and heap state.
    let (psram, psram_free, internal_free) = unsafe {
        (
            sys::esp_psram_get_size(),
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM),
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL),
        )
    };
    log::info!(
        "bring-up: memory at {when}: PSRAM {} KB ({} KB free), internal {} KB free",
        psram / 1024,
        psram_free / 1024,
        internal_free / 1024
    );
    psram
}
