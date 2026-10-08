// ABOUTME: Measures the first second of real LINE IN samples after codec soft setup and I2S restart.
// ABOUTME: Retains bounded block statistics, then prints them after acquisition without judging settling.

use std::time::Duration;

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::units::FromValueType;
use esp_idf_svc::sys;
use esp_shazam_core::audio::samples_from_i2s_slots;

#[path = "../src/codec.rs"]
mod codec;
#[allow(dead_code)]
#[path = "../src/config.rs"]
mod config;
#[path = "../src/i2s_capture.rs"]
mod i2s_capture;
#[path = "capture-settling/stats.rs"]
mod stats;
#[path = "../src/threads.rs"]
mod threads;

use stats::ChannelStats;

const PAIRS: usize = 5;
const TRIALS: usize = PAIRS * 2;
const BLOCKS: usize = 200;
const FRAMES: usize = 240;
const BYTES: usize = FRAMES * 8;
const CAPTURE_DEADLINE_US: u64 = 1_050_000;
const READ_TIMEOUT_MS: u32 = 20;

#[derive(Clone, Copy, Default)]
struct Block {
    completed_us: u64,
    frames: u32,
    read_error: i32,
    queue_overflows: u32,
    malformed: bool,
    late: bool,
    left: ChannelStats,
    right: ChannelStats,
}

#[derive(Clone, Copy, Default)]
struct Trial {
    setup_begin_us: u64,
    codec_end_us: u64,
    restart_begin_us: u64,
    capture_begin_us: u64,
    capture_end_us: u64,
    codec_failed: bool,
    restart_error: i32,
    setup_overflows: u32,
    callback_baseline: u32,
    callback_delta: u32,
    overflow_baseline: u32,
    overflow_delta: u32,
    blocks: usize,
    frames: u32,
    read_errors: u32,
    malformed_reads: u32,
    short_reads: u32,
    late_reads: u32,
    deadline_reached: bool,
}

fn micros() -> u64 {
    // SAFETY: IDF initializes this monotonic timer before main.
    unsafe { sys::esp_timer_get_time() as u64 }
}

fn acquire(
    rx: &mut i2s_capture::CaptureRx<'_>,
    blocks: &mut [Block],
    bytes: &mut [u8; BYTES],
    samples: &mut Vec<i32>,
    trial: &mut Trial,
) {
    let deadline = trial.capture_begin_us + CAPTURE_DEADLINE_US;
    for block in blocks {
        let remaining_ms = deadline.saturating_sub(micros()) / 1_000;
        if remaining_ms == 0 {
            trial.deadline_reached = true;
            break;
        }
        let result = rx.read(bytes, remaining_ms.min(u64::from(READ_TIMEOUT_MS)) as u32);
        block.completed_us = micros();
        block.queue_overflows = i2s_capture::counters()
            .queue_overflows
            .wrapping_sub(trial.overflow_baseline);
        block.late = block.completed_us > deadline;
        trial.late_reads += u32::from(block.late);
        trial.blocks += 1;
        match result {
            Err(error) => {
                block.read_error = error.code();
                trial.read_errors += 1;
            }
            Ok(n) => {
                block.malformed = n > BYTES || n % 8 != 0;
                if block.malformed {
                    trial.malformed_reads += 1;
                } else {
                    block.frames = (n / 8) as u32;
                    trial.frames += block.frames;
                    trial.short_reads += u32::from(block.frames != FRAMES as u32);
                    // Production conversion, with capacity reserved before I2S starts.
                    samples_from_i2s_slots(&bytes[..n], samples);
                    block.left = ChannelStats::measure(samples, 0);
                    block.right = ChannelStats::measure(samples, 1);
                }
            }
        }
        if block.late {
            trial.deadline_reached = true;
            break;
        }
    }
    trial.capture_end_us = micros();
    trial.deadline_reached |= trial.capture_end_us > deadline;
    let counters = i2s_capture::counters();
    trial.callback_delta = counters.callbacks.wrapping_sub(trial.callback_baseline);
    trial.overflow_delta = counters
        .queue_overflows
        .wrapping_sub(trial.overflow_baseline);
}

fn experiment() -> anyhow::Result<()> {
    // One bounded allocation, initialized before any clocks/setup/trials; no hot-path growth.
    let mut blocks = vec![Block::default(); TRIALS * BLOCKS];
    let mut trials = [Trial::default(); TRIALS];
    let mut bytes = [0u8; BYTES];
    let mut samples = Vec::with_capacity(FRAMES * 2);
    // SAFETY: both pointers are within this nonempty live result allocation.
    let storage_psram = unsafe {
        sys::esp_ptr_external_ram(blocks.as_ptr().cast())
            && sys::esp_ptr_external_ram(blocks.as_ptr().add(blocks.len() - 1).cast())
    };
    let peripherals = Peripherals::take()?;
    let mut rx = i2s_capture::CaptureRx::new(
        peripherals.i2s0,
        peripherals.pins.gpio27,
        peripherals.pins.gpio35,
        peripherals.pins.gpio0,
        peripherals.pins.gpio25,
    )?;
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio33,
        peripherals.pins.gpio32,
        &I2cConfig::new().baudrate(100.kHz().into()),
    )?;
    // setup_line_in logs synchronously. Keep that UART work out of all acquisition windows.
    let log_level = log::max_level();
    log::set_max_level(log::LevelFilter::Off);
    let mut codec_ready = false;
    for (index, trial) in trials.iter_mut().enumerate() {
        trial.setup_begin_us = micros();
        let setup_baseline = i2s_capture::counters().queue_overflows;
        if index % 2 == 0 {
            // This is the real soft-reset/setup/readback sequence; not physical power-cold start.
            codec_ready = codec::setup_line_in(&mut i2c, config::CONFIG.pga_gain).is_ok();
        }
        trial.codec_failed = !codec_ready;
        trial.codec_end_us = micros();
        trial.restart_begin_us = micros();
        if let Err(error) = rx.restart() {
            trial.restart_error = error.code();
        }
        trial.capture_begin_us = micros();
        let baseline = i2s_capture::counters();
        trial.setup_overflows = baseline.queue_overflows.wrapping_sub(setup_baseline);
        trial.callback_baseline = baseline.callbacks;
        trial.overflow_baseline = baseline.queue_overflows;
        if !trial.codec_failed && trial.restart_error == 0 {
            acquire(
                &mut rx,
                &mut blocks[index * BLOCKS..(index + 1) * BLOCKS],
                &mut bytes,
                &mut samples,
                trial,
            );
        } else {
            trial.capture_end_us = micros();
        }
    }
    // Stop callbacks before the UART dump; log timestamps below describe recorded events.
    drop(rx);
    log::set_max_level(log_level);
    // UART output can poll continuously. Lower its priority and yield during the dump
    // so the idle task can service its watchdog; recorded acquisition is already complete.
    // SAFETY: a null handle selects this live task; priority 2 is a valid task priority.
    unsafe { sys::vTaskPrioritySet(std::ptr::null_mut(), 2) };
    println!(
        "capture-settling: marker=RESULTS trials={TRIALS} blocks_per_trial={BLOCKS} rate_hz=48000 sample_bits=24 pga_gain={} storage_bytes={} storage_psram={storage_psram} deadline_us={CAPTURE_DEADLINE_US} read_timeout_ms={READ_TIMEOUT_MS} physically_cold=false",
        config::CONFIG.pga_gain,
        blocks.len() * std::mem::size_of::<Block>(),
    );
    for (index, trial) in trials.iter().enumerate() {
        let pair = index / 2 + 1;
        let kind = if index % 2 == 0 {
            "codec-soft-reinit"
        } else {
            "i2s-only-restart"
        };
        println!(
            "capture-settling: marker=BEGIN trial={pair} kind={kind} setup_begin_us={} codec_end_us={} restart_begin_us={} capture_begin_us={} codec_elapsed_us={} restart_elapsed_us={} setup_overflows={} callback_baseline={} overflow_baseline={}",
            trial.setup_begin_us, trial.codec_end_us, trial.restart_begin_us,
            trial.capture_begin_us, trial.codec_end_us - trial.setup_begin_us,
            trial.capture_begin_us - trial.restart_begin_us, trial.setup_overflows,
            trial.callback_baseline, trial.overflow_baseline,
        );
        for (block_index, block) in blocks[index * BLOCKS..index * BLOCKS + trial.blocks]
            .iter()
            .enumerate()
        {
            println!(
                "capture-settling: marker=BLOCK trial={pair} kind={kind} block={block_index} completed_us={} elapsed_us={} frames={} read_error={} queue_overflows={} malformed={} late={} l_min={} l_max={} l_sum={} l_sum_squares={} r_min={} r_max={} r_sum={} r_sum_squares={}",
                block.completed_us, block.completed_us - trial.capture_begin_us,
                block.frames, block.read_error, block.queue_overflows, block.malformed,
                block.late, block.left.min, block.left.max, block.left.sum,
                block.left.sum_squares, block.right.min, block.right.max, block.right.sum,
                block.right.sum_squares,
            );
            if block_index % 16 == 15 {
                // SAFETY: this is task context; one scheduler tick admits the idle task.
                unsafe { sys::vTaskDelay(1) };
            }
        }
        let valid = !trial.codec_failed
            && trial.restart_error == 0
            && trial.blocks == BLOCKS
            && trial.frames == (BLOCKS * FRAMES) as u32
            && trial.read_errors == 0
            && trial.malformed_reads == 0
            && trial.short_reads == 0
            && trial.late_reads == 0
            && !trial.deadline_reached
            && trial.overflow_delta == 0;
        println!(
            "capture-settling: marker=SUMMARY trial={pair} kind={kind} valid={valid} blocks={} frames={} capture_end_us={} elapsed_us={} callback_delta={} overflow_delta={} read_errors={} malformed_reads={} short_reads={} late_reads={} deadline_reached={} codec_failed={} restart_error={}",
            trial.blocks, trial.frames, trial.capture_end_us,
            trial.capture_end_us - trial.capture_begin_us, trial.callback_delta,
            trial.overflow_delta, trial.read_errors, trial.malformed_reads,
            trial.short_reads, trial.late_reads, trial.deadline_reached,
            trial.codec_failed, trial.restart_error,
        );
    }
    println!("capture-settling: marker=END trials={TRIALS}");
    Ok(())
}

fn main() -> anyhow::Result<()> {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    threads::ThreadSpec {
        name: c"capture-settle",
        core: Core::Core0,
        priority: 10,
        stack_bytes: 16 * 1024,
    }
    .spawn(|| {
        if let Err(error) = experiment() {
            println!("capture-settling: marker=FAILED error={error:#}");
        }
    })?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
