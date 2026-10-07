// ABOUTME: Captures LINE IN over I2S at 48 kHz, 24-bit stereo, on its own high-priority thread on core 0.
// ABOUTME: Feeds the stream ring, the optional 12 s recognition ring, and the store's level.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2s::I2s;
use esp_shazam_core::audio::{
    samples_from_i2s_slots, RecognitionBuffer, SampleRing, StereoLevelMeter,
};
use esp_shazam_core::capture_buffer::CaptureBuffer;
use esp_shazam_core::capture_health::{Action, CaptureHealth};
use esp_shazam_core::decimate::Decimator;
use esp_shazam_core::stream_ring::StreamRing;

use crate::config::CONFIG;
use crate::i2s_capture::{self, CaptureRx};
use crate::recognizer::SharedStore;
use crate::threads::ThreadSpec;

/// LINE IN's sample rate, and the stream's.
pub const CAPTURE_RATE_HZ: u32 = 48_000;
/// The rate of the mono feed the Decimator makes for the fingerprint.
pub const FEED_RATE_HZ: u32 = 16_000;
/// The fingerprint's window: the newest 12 s of the feed.
pub const WINDOW_SAMPLES: usize = 12 * FEED_RATE_HZ as usize;
/// Raw frames feed WAV listeners and the shared FLAC encoder. WAV keeps 2 s with Shazam off,
/// and only 0.5 s with it on: the encoded FLAC ring takes the longer network-stall budget.
/// The enabled profile frees 576 KB of raw storage to fund 24 encoded FLAC frame slots.
const STREAM_FRAMES: usize = if CONFIG.shazam_enabled {
    CAPTURE_RATE_HZ as usize / 2
} else {
    2 * CAPTURE_RATE_HZ as usize
};

/// One I2S DMA buffer holds 240 frames (5 ms) of two 32-bit slots: 1,920 bytes. Each read takes one.
const DMA_FRAMES: usize = 240;
const DMA_BYTES: usize = DMA_FRAMES * 2 * 4;
/// The driver queues all but one buffer and silently drops the oldest when the reader falls behind,
/// so 10 buffers let this thread run 45 ms late. The thread never waits on the recognizer, but core 0
/// still slows down: the start of each fingerprint (which fills about 2 MB of fresh PSRAM buffers)
/// put it up to 31 ms behind over 11 asks, and Wi-Fi start-up up to 25 ms at boot; otherwise it ran
/// at most 2.2 ms late. DMA can't reach PSRAM: the buffers take 19,200 bytes of internal RAM.
const READ_TIMEOUT_MS: u32 = 1_000;

/// At most this many feed samples wait for the 12 s ring while the recognizer holds it. A hold lasts
/// about 50 ms, so a second is never reached; past it, the oldest samples go.
const WAITING_LIMIT: usize = FEED_RATE_HZ as usize;

// Staging spans the existing 45 ms DMA budget (17,280 bytes), without allocating on contention.
const STAGING_FRAMES: usize = 45 * CAPTURE_RATE_HZ as usize / 1_000;
const STALL_MS: u64 = 3_000;
const RECOVERY_MS: u64 = 3_000;
static STARTED: AtomicBool = AtomicBool::new(false);
static RESTART: AtomicBool = AtomicBool::new(false);
static FRAMES: AtomicU32 = AtomicU32::new(0);
static DROPPED: AtomicU32 = AtomicU32::new(0);
static READ_ERRORS: AtomicU32 = AtomicU32::new(0);
static RESTARTS: AtomicU32 = AtomicU32::new(0);
static HEALTH_RESTART_REQUESTS: AtomicU32 = AtomicU32::new(0);
static GAPS: AtomicU32 = AtomicU32::new(0);
static RAW_WORK_US: AtomicU32 = AtomicU32::new(0);
static METER_WORK_US: AtomicU32 = AtomicU32::new(0);
static STORE_WORK_US: AtomicU32 = AtomicU32::new(0);
static STAGED_FRAMES: AtomicU32 = AtomicU32::new(0);
static STAGED_MAX_FRAMES: AtomicU32 = AtomicU32::new(0);
static STAGING_PENDING_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
static STAGING_PENDING_MAX_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
static STAGING_PENDING_US: AtomicU32 = AtomicU32::new(0);
static STAGING_PENDING_MAX_US: AtomicU32 = AtomicU32::new(0);

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
) -> Result<CaptureRx<'d>> {
    Ok(CaptureRx::new(i2s, bclk, din, mclk, ws)?)
}

/// Spawns the capture thread. When Shazam is enabled it returns the 12 s recognition ring;
/// it always returns the stream ring of raw frames. Every 250 ms the thread writes
/// the stereo energy of the input's last second into `store`. The first 500 ms are dropped while the
/// codec's reference voltage settles.
pub fn spawn_capture(
    rx: CaptureRx<'static>,
    store: SharedStore,
) -> Result<(Option<SharedRing>, SharedStreamRing)> {
    let recognition = RecognitionBuffer::new(
        CONFIG.shazam_enabled,
        WINDOW_SAMPLES,
        WAITING_LIMIT,
        DMA_FRAMES / 3 + 1,
    );
    let ring = recognition.as_ref().map(RecognitionBuffer::shared_ring);
    let stream: SharedStreamRing = Arc::new(Mutex::new(StreamRing::new(STREAM_FRAMES)));
    let stream_writer = stream.clone();
    ThreadSpec {
        name: c"audio-supervise",
        core: Core::Core1,
        priority: 11,
        stack_bytes: 4 * 1024,
    }
    .spawn(supervise)?;
    // Keep ordinary UART and heap work below the shared encoder's priority.
    ThreadSpec {
        name: c"audio-metrics",
        core: Core::Core1,
        priority: 3,
        stack_bytes: 4 * 1024,
    }
    .spawn(metrics)?;
    ThreadSpec {
        name: c"audio",
        core: Core::Core0,
        priority: 10,
        stack_bytes: 8 * 1024,
    }
    .spawn(move || capture(rx, recognition, stream_writer, store))?;
    Ok((ring, stream))
}

fn capture(
    mut rx: CaptureRx<'static>,
    mut recognition: Option<RecognitionBuffer>,
    stream: SharedStreamRing,
    store: SharedStore,
) -> ! {
    let mut bytes = vec![0u8; DMA_BYTES];
    let mut interleaved: Vec<i32> = Vec::with_capacity(2 * DMA_FRAMES);
    let mut decimator = Decimator::new();
    let mut feed: Vec<i16> = Vec::with_capacity(DMA_FRAMES / 3 + 1);
    let mut meter = StereoLevelMeter::new(CAPTURE_RATE_HZ as usize);
    let mut settle_frames = CAPTURE_RATE_HZ as usize / 2;
    let mut staging = CaptureBuffer::new(STAGING_FRAMES);
    let mut pending_since = None;
    let mut pending_attempts = 0u32;
    // Clocks ran during codec setup; flush startup backlog before the overflow baseline.
    if let Err(error) = rx.restart() {
        log::warn!("audio: initial queue flush failed: {error}");
    }
    let mut overflow_seen = i2s_capture::counters().queue_overflows;
    let mut pending_level = None;
    STARTED.store(true, Ordering::Release);
    loop {
        // A snapshot may have held the recognition ring at the gap. Retry clearing it even
        // while I2S fails or the codec settles, rather than retaining a stale full window.
        if let Some(recognition) = recognition.as_mut() {
            recognition.push(&[]);
        }
        let overflow = i2s_capture::counters().queue_overflows;
        if RESTART.swap(false, Ordering::AcqRel) || overflow != overflow_seen {
            overflow_seen = overflow;
            DROPPED.fetch_add(staging.invalidate() as u32, Ordering::Relaxed);
            staging.publish(&[], &stream);
            record_staging(0, micros(), &mut pending_since, &mut pending_attempts);
            GAPS.fetch_add(1, Ordering::Relaxed);
            if let Some(recognition) = recognition.as_mut() {
                recognition.reset();
            }
            decimator = Decimator::new();
            meter.reset();
            pending_level = None;
            settle_frames = CAPTURE_RATE_HZ as usize / 2;
            RESTARTS.fetch_add(1, Ordering::Relaxed);
            if let Err(error) = rx.restart() {
                log::warn!("audio: restart failed: {error}");
            }
        }
        let n = match rx.read(&mut bytes, READ_TIMEOUT_MS) {
            Ok(n) => n - n % 8, // whole frames only
            Err(_) => {
                READ_ERRORS.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        FRAMES.fetch_add((n / 8) as u32, Ordering::Relaxed);
        // The overflow may have occurred during this read. Its queued audio has an uncertain gap;
        // discard this chunk and flush/restart at the next iteration before publishing again.
        if i2s_capture::counters().queue_overflows != overflow_seen {
            DROPPED.fetch_add((n / 8) as u32, Ordering::Relaxed);
            continue;
        }
        samples_from_i2s_slots(&bytes[..n], &mut interleaved);
        if settle_frames > 0 {
            settle_frames = settle_frames.saturating_sub(interleaved.len() / 2);
            continue;
        }
        let started = micros();
        let dropped = staging.publish(&interleaved, &stream);
        RAW_WORK_US.fetch_max((micros() - started) as u32, Ordering::Relaxed);
        record_staging(
            staging.staged_frames(),
            micros(),
            &mut pending_since,
            &mut pending_attempts,
        );
        if dropped > 0 {
            DROPPED.fetch_add(dropped as u32, Ordering::Relaxed);
            GAPS.fetch_add(1, Ordering::Relaxed);
            if let Some(recognition) = recognition.as_mut() {
                recognition.reset();
            }
            decimator = Decimator::new();
            meter.reset();
            pending_level = None;
        }
        decimator.process(&interleaved, &mut feed);
        if let Some(recognition) = recognition.as_mut() {
            recognition.push(&feed);
        }
        let meter_started = micros();
        if let Some(level) = meter.push(&interleaved) {
            pending_level = Some((level, std::time::Instant::now()));
        }
        METER_WORK_US.fetch_max((micros() - meter_started) as u32, Ordering::Relaxed);
        if let Some((level, measured_at)) = pending_level {
            let started = micros();
            match store.try_lock() {
                Ok(mut store) => {
                    store.set_level(level, measured_at);
                    pending_level = None;
                }
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Poisoned(_)) => panic!("store lock poisoned"),
            }
            STORE_WORK_US.fetch_max((micros() - started) as u32, Ordering::Relaxed);
        }
    }
}

fn micros() -> u64 {
    // ESP-IDF initializes the monotonic timer before main; this getter has no side effects.
    unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 }
}

// Sample after each publish/reset. A streak spans repeated pending publishes, including
// read waits and preemption; it does not imply one reader held the mutex for that duration.
fn record_staging(frames: usize, now: u64, since: &mut Option<u64>, attempts: &mut u32) {
    if frames > 0 {
        let started = *since.get_or_insert(now);
        *attempts = attempts.saturating_add(1);
        let elapsed = now.saturating_sub(started).min(u64::from(u32::MAX)) as u32;
        STAGING_PENDING_US.store(elapsed, Ordering::Relaxed);
        STAGING_PENDING_MAX_US.fetch_max(elapsed, Ordering::Relaxed);
        STAGING_PENDING_MAX_ATTEMPTS.fetch_max(*attempts, Ordering::Relaxed);
    } else {
        if let Some(started) = since.take() {
            let elapsed = now.saturating_sub(started).min(u64::from(u32::MAX)) as u32;
            STAGING_PENDING_MAX_US.fetch_max(elapsed, Ordering::Relaxed);
        }
        *attempts = 0;
        STAGING_PENDING_US.store(0, Ordering::Relaxed);
    }
    STAGING_PENDING_ATTEMPTS.store(*attempts, Ordering::Relaxed);
    STAGED_FRAMES.store(frames as u32, Ordering::Relaxed);
    STAGED_MAX_FRAMES.fetch_max(frames as u32, Ordering::Relaxed);
}

fn supervise() {
    let launched = micros();
    // Startup shares the stall grace even if capture is blocked before STARTED.
    let mut health = CaptureHealth::new(
        STALL_MS,
        RECOVERY_MS,
        launched / 1_000,
        FRAMES.load(Ordering::Relaxed),
    );
    loop {
        match health.observe(micros() / 1_000, FRAMES.load(Ordering::Relaxed)) {
            Action::None => {}
            Action::Restart => {
                RESTART.store(true, Ordering::Release);
                HEALTH_RESTART_REQUESTS.fetch_add(1, Ordering::Relaxed);
            }
            Action::Reboot => {
                // Do not wait for asynchronous metrics/UART. IDF's restart internals can
                // still wait on Wi-Fi/locks; this call is not a hard reboot-time guarantee.
                unsafe { esp_idf_svc::sys::esp_restart() }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn metrics() {
    // Sole owner of IDF's process-global minimum-heap monitor. Allocate its small saved
    // minima array once at boot, before measuring rolling windows under listener load.
    let mut window_start_free = start_heap_window();
    let launched = micros();
    let mut epoch = None;
    let mut previous = FRAMES.load(Ordering::Relaxed);
    let mut observed = 0u64;
    let mut requests_seen = 0;
    let mut next_log = launched + 10_000_000;
    loop {
        let now = micros();
        let frames = FRAMES.load(Ordering::Relaxed);
        if epoch.is_none() && STARTED.load(Ordering::Acquire) {
            epoch = Some(now);
            observed = 0;
        } else if epoch.is_some() {
            observed += u64::from(frames.wrapping_sub(previous));
        }
        previous = frames;
        let requests = HEALTH_RESTART_REQUESTS.load(Ordering::Relaxed);
        if requests != requests_seen {
            requests_seen = requests;
            // Event delivery is asynchronous and may lag the capture owner's restart.
            log::warn!("audio: no captured-frame progress for {STALL_MS} ms; requesting restart (asynchronously observed; health_restart_requests={requests})");
        }
        if now >= next_log {
            if let Some(epoch) = epoch {
                log_capture(now - epoch, observed, &mut window_start_free);
            }
            next_log = now + 10_000_000;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn start_heap_window() -> [usize; 2] {
    use esp_idf_svc::sys;
    // SAFETY: only the metrics task calls this process-global API. In pinned IDF 5.5.5,
    // repeated start resets local minima and retains metadata, without allocating again
    // (heap_caps.c:314-343), despite the header saying an active start would fail.
    unsafe {
        assert_eq!(
            sys::heap_caps_monitor_local_minimum_free_size_start(),
            sys::ESP_OK
        );
        [
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT),
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT),
        ]
    }
}

fn log_capture(elapsed_us: u64, observed: u64, window_start_free: &mut [usize; 2]) {
    use esp_idf_svc::sys;
    let counters = i2s_capture::counters();
    let internal = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
    let external = sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT;
    // Minimum sums combine per-region lows that may occur at different times: a conservative
    // bound on aggregate free memory. Largest blocks are snapshots, not window minima.
    // Heap statistics and monitor resets run outside the capture task.
    let (free, largest, psram_free, psram_largest, internal_min_sum, psram_min_sum) = unsafe {
        (
            sys::heap_caps_get_free_size(internal),
            sys::heap_caps_get_largest_free_block(internal),
            sys::heap_caps_get_free_size(external),
            sys::heap_caps_get_largest_free_block(external),
            sys::heap_caps_get_minimum_free_size(internal),
            sys::heap_caps_get_minimum_free_size(external),
        )
    };
    let start_free = *window_start_free;
    // Logging overhead belongs to the next window. No stop/start allocation cycle.
    *window_start_free = start_heap_window();
    let deficit = (elapsed_us * u64::from(CAPTURE_RATE_HZ) / 1_000_000) as i64 - observed as i64;
    log::info!("audio: frames={} callbacks={} queue_overflows={} software_dropped_frames={} read_errors={} restarts={} discontinuities={} health_restart_requests={} raw_work_max_us={} meter_work_max_us={} store_work_max_us={} raw_read_max_us={} staged_frames={} staged_max_frames={} staging_pending_attempts={} staging_pending_max_attempts={} staging_pending_us={} staging_pending_max_us={} timing_deficit_estimate_frames={} (clock drift/scheduling; not proof of loss) internal_free={} internal_largest={} psram_free={} psram_largest={} internal_min_sum={} psram_min_sum={} internal_window_start_free={} psram_window_start_free={} (minima sum per-region lows; largest is a snapshot; staging current values sampled at publish/reset)",
        FRAMES.load(Ordering::Relaxed), counters.callbacks, counters.queue_overflows,
        DROPPED.load(Ordering::Relaxed), READ_ERRORS.load(Ordering::Relaxed),
        RESTARTS.load(Ordering::Relaxed), GAPS.load(Ordering::Relaxed),
        HEALTH_RESTART_REQUESTS.load(Ordering::Relaxed),
        RAW_WORK_US.swap(0, Ordering::Relaxed), METER_WORK_US.swap(0, Ordering::Relaxed), STORE_WORK_US.swap(0, Ordering::Relaxed),
        crate::stream::take_raw_read_max_us(), STAGED_FRAMES.load(Ordering::Relaxed),
        STAGED_MAX_FRAMES.swap(0, Ordering::Relaxed),
        STAGING_PENDING_ATTEMPTS.load(Ordering::Relaxed), STAGING_PENDING_MAX_ATTEMPTS.swap(0, Ordering::Relaxed),
        STAGING_PENDING_US.load(Ordering::Relaxed), STAGING_PENDING_MAX_US.swap(0, Ordering::Relaxed),
        deficit, free, largest, psram_free, psram_largest, internal_min_sum, psram_min_sum,
        start_free[0], start_free[1]);
}
