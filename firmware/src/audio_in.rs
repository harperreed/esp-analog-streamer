// ABOUTME: Captures LINE IN over I2S at 16 kHz, averages left and right to mono, and keeps the newest 12 s.
// ABOUTME: Runs on its own high-priority thread on core 0; everything else reads the shared ring.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::delay::TickType;
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2s::config::{
    Config, DataBitWidth, SlotMode, StdClkConfig, StdConfig, StdGpioConfig, StdSlotConfig,
};
use esp_idf_svc::hal::i2s::{I2s, I2sDriver, I2sRx};
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_shazam_core::audio::{stereo_to_mono, SampleRing};

pub const SAMPLE_RATE_HZ: u32 = 16_000;
pub const WINDOW_SAMPLES: usize = 12 * SAMPLE_RATE_HZ as usize;

/// I2S DMA buffers of 240 frames (15 ms) each. The driver queues all but one and drops the oldest
/// when the reader falls behind, so 16 let the capture thread wait 225 ms for the ring lock.
/// The recognizer's 12 s snapshot holds that lock for about 90 ms; the default of 6 allowed only 75 ms.
const DMA_BUFFER_COUNT: u32 = 16;

pub type SharedRing = Arc<Mutex<SampleRing>>;

/// I2S RX with clocks running (MCLK 256 x 16 kHz = 4.096 MHz on GPIO0, BCLK 512 kHz).
/// Start it before the codec setup.
pub fn start_i2s<'d>(
    i2s: impl I2s + 'd,
    bclk: impl InputPin + OutputPin + 'd,
    din: impl InputPin + 'd,
    mclk: impl InputPin + OutputPin + 'd,
    ws: impl InputPin + OutputPin + 'd,
) -> Result<I2sDriver<'d, I2sRx>> {
    // StdConfig::philips with a deeper DMA queue.
    let config = StdConfig::new(
        Config::default().dma_buffer_count(DMA_BUFFER_COUNT),
        StdClkConfig::from_sample_rate_hz(SAMPLE_RATE_HZ),
        StdSlotConfig::philips_slot_default(DataBitWidth::Bits16, SlotMode::Stereo),
        StdGpioConfig::default(),
    );
    let mut rx = I2sDriver::<I2sRx>::new_std_rx(i2s, &config, bclk, din, Some(mclk), ws)?;
    rx.rx_enable()?; // mandatory: read() needs the channel running
    Ok(rx)
}

/// Spawns the capture thread and returns the ring it fills. The first 500 ms are dropped
/// while the codec's reference voltage settles.
pub fn spawn_capture(mut rx: I2sDriver<'static, I2sRx>) -> Result<SharedRing> {
    let ring: SharedRing = Arc::new(Mutex::new(SampleRing::new(WINDOW_SAMPLES)));
    let writer = ring.clone();
    ThreadSpawnConfiguration {
        name: Some(c"audio"),
        priority: 10,
        pin_to_core: Some(Core::Core0),
        ..Default::default()
    }
    .set()?;
    // Only Builder::stack_size sets a std thread's stack; ThreadSpawnConfiguration's is ignored.
    let spawned = std::thread::Builder::new()
        .stack_size(8 * 1024)
        .spawn(move || {
            let mut bytes = vec![0u8; 1280]; // 20 ms of 16-bit stereo
            let mut frames: Vec<i16> = Vec::with_capacity(640);
            let mut mono: Vec<i16> = Vec::with_capacity(320);
            let mut settle = SAMPLE_RATE_HZ as usize / 2;
            loop {
                let n = match rx.read(&mut bytes, TickType::new_millis(1000).ticks()) {
                    Ok(n) => n - n % 4, // whole stereo frames only
                    Err(e) => {
                        log::warn!("audio: I2S read failed: {e}");
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                };
                frames.clear();
                frames.extend(
                    bytes[..n]
                        .chunks_exact(2)
                        .map(|b| i16::from_le_bytes([b[0], b[1]])),
                );
                stereo_to_mono(&frames, &mut mono);
                if settle > 0 {
                    settle = settle.saturating_sub(mono.len());
                    continue;
                }
                writer.lock().expect("audio ring lock").push(&mono);
            }
        });
    ThreadSpawnConfiguration::default().set()?;
    spawned?;
    Ok(ring)
}
