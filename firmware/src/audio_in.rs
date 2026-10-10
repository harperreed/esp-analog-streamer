// ABOUTME: Captures LINE IN over I2S at 48 kHz, 24-bit stereo, on its own high-priority thread on core 0.
// ABOUTME: Feeds the stream ring, the optional 12 s recognition ring, and the store's level.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2s::I2s;
use esp_shazam_core::audio::{samples_from_i2s_slots, SampleRing, StereoLevelMeter};
use esp_shazam_core::capture_buffer::CaptureBuffer;
use esp_shazam_core::capture_health::{Action, CaptureHealth};
use esp_shazam_core::stream_ring::StreamRing;

use crate::capture_failure_telemetry::{ExportState, FailureKind, Handoff, Summary};
use crate::config::CONFIG;
use crate::i2s_capture::{self, CaptureRx};
use crate::recognizer::SharedStore;
use crate::threads::ThreadSpec;

#[path = "capture_timing.rs"]
mod capture_timing;
use capture_timing::ReadServiceTiming;
#[path = "recognition_feed.rs"]
mod recognition_feed;
use recognition_feed::RecognitionFeed;

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

const STALL_MS: u64 = 3_000;
const RECOVERY_MS: u64 = 3_000;
static STARTED: AtomicBool = AtomicBool::new(false);
static RESTART: AtomicBool = AtomicBool::new(false);
static RESTART_FAILURES: Handoff = Handoff::new();
static FRAMES: AtomicU32 = AtomicU32::new(0);
static DROPPED: AtomicU32 = AtomicU32::new(0);
static READ_ERRORS: AtomicU32 = AtomicU32::new(0);
static RESTARTS: AtomicU32 = AtomicU32::new(0);
static SUPERVISOR_RESTARTS: AtomicU32 = AtomicU32::new(0);
static OVERFLOW_RESTARTS: AtomicU32 = AtomicU32::new(0);
static SETTLING_DISCARDED_FRAMES: AtomicU32 = AtomicU32::new(0);
static READ_SERVICE_DELAY_US: AtomicU32 = AtomicU32::new(0);
// The capture owner never returns or deletes its task; metrics only reads its runtime counter.
static CAPTURE_TASK: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
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
pub type SharedStreamRing = Arc<StreamRing>;

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
    let recognition = RecognitionFeed::new(
        CONFIG.shazam_enabled,
        WINDOW_SAMPLES,
        WAITING_LIMIT,
        DMA_FRAMES,
    );
    let ring = recognition.shared_ring();
    let (publisher, stream) = CaptureBuffer::new(STREAM_FRAMES, DMA_FRAMES);
    verify_stream_placement(&stream)?;
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
    .spawn(move || capture(rx, recognition, publisher, store))?;
    Ok((ring, stream))
}

// Checked identity exhaustion cannot safely continue publication. Logging can wait on a
// task-owned lock, so fatal recovery goes straight to the existing restart path.
fn publication_exhausted() -> ! {
    // SAFETY: ESP-IDF restart is the existing nonreturning fatal recovery operation.
    unsafe { esp_idf_svc::sys::esp_restart() }
}

fn internal_byte_ram(pointer: *const c_void) -> bool {
    // Pinned ESP32 IDF esp_memory_utils.c:50-76 admits internal byte RAM and PSRAM;
    // the external-RAM predicate removes PSRAM. Both functions have generated bindings.
    // SAFETY: IDF predicates inspect only the pointer address and never dereference it.
    unsafe {
        esp_idf_svc::sys::esp_ptr_byte_accessible(pointer)
            && !esp_idf_svc::sys::esp_ptr_external_ram(pointer)
    }
}

fn verify_stream_placement(stream: &SharedStreamRing) -> Result<()> {
    let (payload, payload_bytes) = stream.payload_range();
    let (stamps, stamps_bytes) = stream.stamps_range();
    let metadata = Arc::as_ptr(stream).cast::<u8>();
    // SAFETY: these nonempty ranges belong to live fixed allocations. IDF heap allocations
    // stay within one memory region, so the internal Arc object also keeps its counts internal.
    let valid = unsafe {
        esp_idf_svc::sys::esp_ptr_external_ram(payload.cast())
            && esp_idf_svc::sys::esp_ptr_external_ram(
                payload.cast::<u8>().add(payload_bytes - 1).cast(),
            )
            && internal_byte_ram(stamps.cast())
            && internal_byte_ram(stamps.cast::<u8>().add(stamps_bytes - 1).cast())
            && internal_byte_ram(metadata.cast())
            && internal_byte_ram(metadata.add(std::mem::size_of::<StreamRing>() - 1).cast())
    };
    anyhow::ensure!(
        valid,
        "capture history requires PSRAM payload and internal metadata"
    );
    Ok(())
}

fn capture(
    mut rx: CaptureRx<'static>,
    mut recognition: RecognitionFeed,
    mut publisher: CaptureBuffer,
    store: SharedStore,
) -> ! {
    let mut bytes = vec![0u8; DMA_BYTES];
    let mut interleaved: Vec<i32> = Vec::with_capacity(2 * DMA_FRAMES);
    let mut meter = StereoLevelMeter::new(CAPTURE_RATE_HZ as usize);
    let mut settle_frames = CAPTURE_RATE_HZ as usize / 2;
    let mut pending_since = None;
    let mut pending_attempts = 0u32;
    let mut read_timing = ReadServiceTiming::new(CAPTURE_RATE_HZ);
    // Clocks ran during codec setup; flush startup backlog before the overflow baseline.
    RESTART_FAILURES.observe_restart_result(
        FailureKind::InitialFlush,
        rx.restart().map_err(|error| error.code()),
    );
    let mut overflow_seen = i2s_capture::counters().queue_overflows;
    let mut pending_level = None;
    // SAFETY: this is the live capture task, whose function never returns.
    CAPTURE_TASK.store(
        unsafe { esp_idf_svc::sys::xTaskGetCurrentTaskHandle().cast() },
        Ordering::Release,
    );
    STARTED.store(true, Ordering::Release);
    loop {
        // A snapshot may have held the recognition ring at the gap. Retry clearing it even
        // while I2S fails or the codec settles, rather than retaining a stale full window.
        recognition.retry_reset();
        let overflow = i2s_capture::counters().queue_overflows;
        let supervisor_restart = RESTART.swap(false, Ordering::AcqRel);
        let overflow_restart = overflow != overflow_seen;
        if supervisor_restart || overflow_restart {
            SUPERVISOR_RESTARTS.fetch_add(u32::from(supervisor_restart), Ordering::Relaxed);
            OVERFLOW_RESTARTS.fetch_add(u32::from(overflow_restart), Ordering::Relaxed);
            read_timing.reset();
            overflow_seen = overflow;
            let discarded = publisher
                .invalidate()
                .unwrap_or_else(|_| publication_exhausted());
            DROPPED.fetch_add(discarded as u32, Ordering::Relaxed);
            record_staging(0, micros(), &mut pending_since, &mut pending_attempts);
            GAPS.fetch_add(1, Ordering::Relaxed);
            recognition.reset();
            meter.reset();
            pending_level = None;
            settle_frames = CAPTURE_RATE_HZ as usize / 2;
            RESTARTS.fetch_add(1, Ordering::Relaxed);
            RESTART_FAILURES.observe_restart_result(
                FailureKind::RecoveryRestart,
                rx.restart().map_err(|error| error.code()),
            );
        }
        let n = match rx.read(&mut bytes, READ_TIMEOUT_MS) {
            Ok(n) => n - n % 8, // whole frames only
            Err(_) => {
                read_timing.reset();
                READ_ERRORS.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        READ_SERVICE_DELAY_US.fetch_max(read_timing.complete(micros(), n / 8), Ordering::Relaxed);
        FRAMES.fetch_add((n / 8) as u32, Ordering::Relaxed);
        // The overflow may have occurred during this read. Its queued audio has an uncertain gap;
        // discard this chunk and flush/restart at the next iteration before publishing again.
        if i2s_capture::counters().queue_overflows != overflow_seen {
            DROPPED.fetch_add((n / 8) as u32, Ordering::Relaxed);
            continue;
        }
        samples_from_i2s_slots(&bytes[..n], &mut interleaved);
        if settle_frames > 0 {
            SETTLING_DISCARDED_FRAMES.fetch_add((interleaved.len() / 2) as u32, Ordering::Relaxed);
            settle_frames = settle_frames.saturating_sub(interleaved.len() / 2);
            continue;
        }
        let started = micros();
        publisher
            .publish(&interleaved)
            .unwrap_or_else(|_| publication_exhausted());
        RAW_WORK_US.fetch_max((micros() - started) as u32, Ordering::Relaxed);
        record_staging(
            publisher.staged_frames(),
            micros(),
            &mut pending_since,
            &mut pending_attempts,
        );
        recognition.process(&interleaved);
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

// Staging fields track only unpublished partial-block assembly after each publish/reset.
// Pending time includes I2S waits and preemption; readers cannot hold publication back.
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
    let mut runtime_sample = None;
    let mut failure_exports = ExportState::new();
    loop {
        let now = micros();
        let frames = FRAMES.load(Ordering::Relaxed);
        if epoch.is_none() && STARTED.load(Ordering::Acquire) {
            epoch = Some(now);
            observed = 0;
            runtime_sample = capture_runtime();
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
                log_capture(
                    now - epoch,
                    observed,
                    &mut window_start_free,
                    &mut runtime_sample,
                );
            }
            export_restart_failures(&mut failure_exports);
            next_log = now + 10_000_000;
        }
        crate::stream::export_listener_events();
        crate::stream::export_encoder_events();
        std::thread::sleep(Duration::from_millis(100));
    }
}

// Aggregate rare driver failures after capture reporting returns, outside the owner path.
#[inline(never)]
fn export_restart_failures(state: &mut ExportState) {
    for summary in state
        .select(RESTART_FAILURES.snapshot())
        .into_iter()
        .flatten()
    {
        log_restart_failure(&summary);
    }
}

#[inline(never)]
fn log_restart_failure(summary: &Summary) {
    let operation = match summary.kind {
        FailureKind::InitialFlush => "initial queue flush",
        FailureKind::RecoveryRestart => "restart",
    };
    log::warn!("audio: {operation} failed: failures_total={} failures_delta={} sampled_error_code={} export_us={} aggregate=true count_code_coherent=false",
        summary.failures_total, summary.failures_delta, summary.sampled_error_code, micros());
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

fn capture_runtime() -> Option<(u64, u32)> {
    use esp_idf_svc::sys;
    let task = CAPTURE_TASK.load(Ordering::Acquire);
    if task.is_null() {
        return None;
    }
    let mut status = sys::TaskStatus_t::default();
    // SAFETY: capture publishes its permanently live task. pdFALSE skips stack scanning;
    // eRunning skips state lookup. Pinned IDF reads the counter without heap allocation.
    unsafe {
        sys::vTaskGetInfo(task.cast(), &mut status, 0, sys::eTaskState_eRunning);
    }
    Some((micros(), status.ulRunTimeCounter))
}

// Keep capture formatting's stack frame separate from listener export in metrics.
#[inline(never)]
fn log_capture(
    elapsed_us: u64,
    observed: u64,
    window_start_free: &mut [usize; 2],
    runtime_sample: &mut Option<(u64, u32)>,
) {
    use esp_idf_svc::sys;
    let capture_begin_us = micros();
    let counters = i2s_capture::counters();
    let frames = FRAMES.load(Ordering::Relaxed);
    let dropped = DROPPED.load(Ordering::Relaxed);
    let read_errors = READ_ERRORS.load(Ordering::Relaxed);
    let restarts = RESTARTS.load(Ordering::Relaxed);
    let supervisor_restarts = SUPERVISOR_RESTARTS.load(Ordering::Relaxed);
    let overflow_restarts = OVERFLOW_RESTARTS.load(Ordering::Relaxed);
    let settling_discarded_frames = SETTLING_DISCARDED_FRAMES.load(Ordering::Relaxed);
    let gaps = GAPS.load(Ordering::Relaxed);
    let health_restart_requests = HEALTH_RESTART_REQUESTS.load(Ordering::Relaxed);
    let capture_end_us = micros();
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
    let current_runtime = capture_runtime();
    let (runtime_window_us, task_runtime_us, runtime_valid) =
        match (*runtime_sample, current_runtime) {
            (Some((previous_us, previous_runtime)), Some((now_us, runtime))) => (
                now_us - previous_us,
                runtime.wrapping_sub(previous_runtime),
                now_us <= u64::from(u32::MAX),
            ),
            _ => (0, 0, false),
        };
    *runtime_sample = current_runtime;
    // Logging overhead belongs to the next window. No stop/start allocation cycle.
    *window_start_free = start_heap_window();
    let deficit = (elapsed_us * u64::from(CAPTURE_RATE_HZ) / 1_000_000) as i64 - observed as i64;
    log::info!("audio: frames={} capture_begin_us={} capture_end_us={} callbacks={} queue_overflows={} software_dropped_frames={} read_errors={} restarts={} supervisor_restarts={} overflow_restarts={} settling_discarded_frames={} discontinuities={} health_restart_requests={} capture_runtime_window_us={} capture_task_runtime_us={} capture_runtime_valid={} read_service_delay_estimate_max_us={} raw_work_max_us={} meter_work_max_us={} store_work_max_us={} raw_read_max_us={} staged_frames={} staged_max_frames={} staging_pending_attempts={} staging_pending_max_attempts={} staging_pending_us={} staging_pending_max_us={} timing_deficit_estimate_frames={} (clock drift/scheduling; not proof of loss) internal_free={} internal_largest={} psram_free={} psram_largest={} internal_min_sum={} psram_min_sum={} internal_window_start_free={} psram_window_start_free={} (runtime includes charged interrupts and excludes unfinished slice; service delay includes work/preemption; minima sum per-region lows; largest is a snapshot; staging fields measure partial block assembly, sampled at publish/reset)",
        frames, capture_begin_us, capture_end_us, counters.callbacks, counters.queue_overflows,
        dropped, read_errors,
        restarts,
        supervisor_restarts, overflow_restarts,
        settling_discarded_frames,
        gaps,
        health_restart_requests,
        runtime_window_us, task_runtime_us, runtime_valid,
        READ_SERVICE_DELAY_US.swap(0, Ordering::Relaxed),
        RAW_WORK_US.swap(0, Ordering::Relaxed), METER_WORK_US.swap(0, Ordering::Relaxed), STORE_WORK_US.swap(0, Ordering::Relaxed),
        crate::stream::take_raw_read_max_us(), STAGED_FRAMES.load(Ordering::Relaxed),
        STAGED_MAX_FRAMES.swap(0, Ordering::Relaxed),
        STAGING_PENDING_ATTEMPTS.load(Ordering::Relaxed), STAGING_PENDING_MAX_ATTEMPTS.swap(0, Ordering::Relaxed),
        STAGING_PENDING_US.load(Ordering::Relaxed), STAGING_PENDING_MAX_US.swap(0, Ordering::Relaxed),
        deficit, free, largest, psram_free, psram_largest, internal_min_sum, psram_min_sum,
        start_free[0], start_free[1]);
}
