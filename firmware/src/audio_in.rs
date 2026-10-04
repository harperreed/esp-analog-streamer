// ABOUTME: Captures LINE IN over I2S at 48 kHz, 24-bit stereo, on its own high-priority thread on core 0.
// ABOUTME: Feeds the 2 s stream ring, the 12 s 16 kHz mono ring the recognizer reads, and the store's level.

use std::sync::{Arc, Mutex, TryLockError};
use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::delay::TickType;
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2s::config::{
    ClockSource, Config, DataBitWidth, MclkMultiple, SlotMode, StdClkConfig, StdConfig,
    StdGpioConfig, StdSlotConfig,
};
use esp_idf_svc::hal::i2s::{I2s, I2sDriver, I2sRx};
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_shazam_core::audio::{samples_from_i2s_slots, LevelMeter, SampleRing};
use esp_shazam_core::decimate::Decimator;
use esp_shazam_core::stream_ring::StreamRing;

use crate::recognizer::SharedStore;

/// LINE IN's sample rate, and the stream's.
pub const CAPTURE_RATE_HZ: u32 = 48_000;
/// The rate of the mono feed the Decimator makes for the fingerprint and the level meter.
pub const FEED_RATE_HZ: u32 = 16_000;
/// The fingerprint's window: the newest 12 s of the feed.
pub const WINDOW_SAMPLES: usize = 12 * FEED_RATE_HZ as usize;
/// The stream ring keeps the newest 2 s of frames: 768,000 bytes of PSRAM.
const STREAM_FRAMES: usize = 2 * CAPTURE_RATE_HZ as usize;

/// One I2S DMA buffer holds 240 frames (5 ms) of two 32-bit slots: 1,920 bytes. Each read takes one.
const DMA_FRAMES: usize = 240;
const DMA_BYTES: usize = DMA_FRAMES * 2 * 4;
/// The driver queues all but one buffer and silently drops the oldest when the reader falls behind,
/// so 10 buffers let this thread run 45 ms late. The thread never waits on the recognizer, but core 0
/// still slows down: the start of each fingerprint (which fills about 2 MB of fresh PSRAM buffers)
/// put it up to 31 ms behind over 9 asks, and Wi-Fi start-up up to 25 ms at boot; otherwise 7 ms at
/// most. DMA can't reach PSRAM: the buffers take 19,200 bytes of internal RAM.
const DMA_BUFFER_COUNT: u32 = 10;

/// The level is the loudness of the feed's last second, measured every 250 ms.
const METER_WINDOW: usize = FEED_RATE_HZ as usize;
const METER_INTERVAL: usize = FEED_RATE_HZ as usize / 4;
/// At most this many feed samples wait for the 12 s ring while the recognizer holds it. A hold lasts
/// about 50 ms, so a second is never reached; past it, the oldest samples go.
const WAITING_LIMIT: usize = FEED_RATE_HZ as usize;

pub type SharedRing = Arc<Mutex<SampleRing>>;
pub type SharedStreamRing = Arc<Mutex<StreamRing>>;

/// I2S RX with clocks running. Start it before the codec setup.
/// - MCLK on GPIO0 is 256 x 48 kHz = 12.288 MHz, from the APLL. ESP-IDF sets the APLL to twice that
///   and divides by 2. The default 160 MHz PLL would need a fractional divider (13 1/48), which jitters.
/// - Each frame is two 32-bit slots (BCLK 3.072 MHz), and the codec's 24-bit samples arrive in the
///   top 24 bits of each slot. On the classic ESP32 a slot has as many bit clocks as the data width,
///   so 32-bit slots take a 32-bit data width.
pub fn start_i2s<'d>(
    i2s: impl I2s + 'd,
    bclk: impl InputPin + OutputPin + 'd,
    din: impl InputPin + 'd,
    mclk: impl InputPin + OutputPin + 'd,
    ws: impl InputPin + OutputPin + 'd,
) -> Result<I2sDriver<'d, I2sRx>> {
    let config = StdConfig::new(
        Config::default()
            .dma_buffer_count(DMA_BUFFER_COUNT)
            .frames_per_buffer(DMA_FRAMES as u32),
        StdClkConfig::new(CAPTURE_RATE_HZ, ClockSource::Apll, MclkMultiple::M256),
        StdSlotConfig::philips_slot_default(DataBitWidth::Bits32, SlotMode::Stereo),
        StdGpioConfig::default(),
    );
    let mut rx = I2sDriver::<I2sRx>::new_std_rx(i2s, &config, bclk, din, Some(mclk), ws)?;
    rx.rx_enable()?; // mandatory: read() needs the channel running
    Ok(rx)
}

/// Spawns the capture thread. It returns the 12 s ring of the 16 kHz feed, which the recognizer
/// fingerprints, and the 2 s ring of raw frames for stream listeners. Every 250 ms the thread writes
/// the loudness of the feed's last second into `store`. The first 500 ms are dropped while the
/// codec's reference voltage settles.
pub fn spawn_capture(
    rx: I2sDriver<'static, I2sRx>,
    store: SharedStore,
) -> Result<(SharedRing, SharedStreamRing)> {
    let ring: SharedRing = Arc::new(Mutex::new(SampleRing::new(WINDOW_SAMPLES)));
    let stream: SharedStreamRing = Arc::new(Mutex::new(StreamRing::new(STREAM_FRAMES)));
    let (ring_writer, stream_writer) = (ring.clone(), stream.clone());
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
        .spawn(move || capture(rx, ring_writer, stream_writer, store));
    ThreadSpawnConfiguration::default().set()?;
    spawned?;
    Ok((ring, stream))
}

fn capture(
    mut rx: I2sDriver<'static, I2sRx>,
    ring: SharedRing,
    stream: SharedStreamRing,
    store: SharedStore,
) -> ! {
    let mut bytes = vec![0u8; DMA_BYTES];
    let mut interleaved: Vec<i32> = Vec::with_capacity(2 * DMA_FRAMES);
    let mut decimator = Decimator::new();
    let mut feed: Vec<i16> = Vec::with_capacity(DMA_FRAMES / 3 + 1);
    let mut waiting: Vec<i16> = Vec::with_capacity(WAITING_LIMIT + feed.capacity());
    let mut meter = LevelMeter::new(METER_WINDOW, METER_INTERVAL);
    let mut settle_frames = CAPTURE_RATE_HZ as usize / 2;
    loop {
        let n = match rx.read(&mut bytes, TickType::new_millis(1000).ticks()) {
            Ok(n) => n - n % 8, // whole frames only
            Err(e) => {
                log::warn!("audio: I2S read failed: {e}");
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        samples_from_i2s_slots(&bytes[..n], &mut interleaved);
        if settle_frames > 0 {
            settle_frames = settle_frames.saturating_sub(interleaved.len() / 2);
            continue;
        }
        stream.lock().expect("stream ring lock").push(&interleaved);
        decimator.process(&interleaved, &mut feed);
        waiting.extend_from_slice(&feed);
        match ring.try_lock() {
            Ok(mut ring) => {
                ring.push(&waiting);
                waiting.clear();
            }
            // The recognizer is copying its 12 s window out (about 50 ms). Waiting for the lock
            // would overrun the DMA queue, so the samples wait here for a later chunk instead.
            Err(TryLockError::WouldBlock) => {
                let excess = waiting.len().saturating_sub(WAITING_LIMIT);
                waiting.drain(..excess);
            }
            Err(TryLockError::Poisoned(_)) => panic!("audio ring lock poisoned"),
        }
        if let Some(level) = meter.push(&feed) {
            store.lock().expect("store lock").set_level(level);
        }
    }
}
