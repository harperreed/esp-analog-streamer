// ABOUTME: Measures real-device FLAC costs for deterministic zero, DC, mixed, and noise blocks.
// ABOUTME: Runs on core 1 without capture or network startup; reports warmup separately.

// Build from firmware/: cargo +esp build --release --example flac-cost.
// Task runtime needs the project's enabled FreeRTOS ESP_TIMER statistics. It excludes sleep,
// but includes interrupt time charged to this task and the unfinished-slice sampling error.
// encoding_us covers reset, chunk copies, push and encode, including any interruption there.

use std::time::Duration;

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys;
use esp_shazam_core::flac::{Encoder, BLOCK_FRAMES, CHANNELS, MAX_FRAME_BYTES};

#[path = "flac-cost/contract.rs"]
mod contract;
#[path = "../src/threads.rs"]
mod threads;

const BLOCKS: usize = 128;
const CHUNK_SAMPLES: usize = 240 * CHANNELS;

fn main() -> anyhow::Result<()> {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let (finished, result) = std::sync::mpsc::channel();
    threads::ThreadSpec {
        name: c"flac-cost",
        core: Core::Core1,
        priority: 7,
        stack_bytes: 4 * 1024,
    }
    .spawn(move || {
        benchmark();
        finished.send(()).expect("benchmark result receiver");
    })?;
    result.recv()?;
    log::info!("flac-cost: assertions passed; no capture, Wi-Fi or Shazam started");
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn benchmark() {
    let mut encoder = Encoder::new();
    let mut input = vec![0i32; BLOCK_FRAMES * CHANNELS];
    let mut chunk = vec![0i32; CHUNK_SAMPLES];
    let mut frame: Vec<u8> = Vec::with_capacity(MAX_FRAME_BYTES);
    // SAFETY: these live allocations have readable pointers; the IDF getter only checks addresses.
    unsafe {
        log::info!(
            "flac-cost: input_psram={} chunk_psram={} frame_psram={} blocks={} header_number=0",
            sys::esp_ptr_external_ram(input.as_ptr().cast()),
            sys::esp_ptr_external_ram(chunk.as_ptr().cast()),
            sys::esp_ptr_external_ram(frame.as_ptr().cast()),
            BLOCKS
        );
    }
    for kind in ["zero", "dc", "mixed", "noise"] {
        let mut state = 0x1234_5678u32;
        for pair in input.chunks_exact_mut(CHANNELS) {
            for (channel, sample) in pair.iter_mut().enumerate() {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *sample = match kind {
                    "zero" => 0,
                    "dc" => {
                        if channel == 0 {
                            123_456
                        } else {
                            -123_456
                        }
                    }
                    "mixed" if channel == 0 => 123_456,
                    _ => (state as i32) >> 8,
                };
            }
        }
        for (phase, blocks) in [("warmup", 1), ("measured", BLOCKS)] {
            // Commit setup/previous logging runtime before this phase's baseline snapshot.
            std::thread::sleep(Duration::from_millis(10));
            let initial_ptr = frame.as_ptr();
            let initial_capacity = frame.capacity();
            let cpu_start = runtime_us();
            let wall_start = now_us();
            let mut encoding_us = 0;
            let mut bytes = 0;
            let mut largest = 0;
            for _ in 0..blocks {
                let started = now_us();
                encoder.reset();
                // Match production's small internal-RAM chunk feeding, instead of sweeping
                // the full PSRAM input directly from the encoder's predictor loop.
                for samples in input.chunks(CHUNK_SAMPLES) {
                    chunk[..samples.len()].copy_from_slice(samples);
                    encoder.push(&chunk[..samples.len()]);
                }
                encoder.encode_block(0, &mut frame);
                encoding_us += now_us() - started;
                contract::assert_frame(kind, &frame, initial_ptr, initial_capacity);
                bytes += frame.len();
                largest = largest.max(frame.len());
                // Yield between blocks so the scheduler commits this task's runtime counter.
                // This delay is included in wall_us, but excluded from encoding_us.
                std::thread::sleep(Duration::from_millis(10));
            }
            let cpu_end = runtime_us();
            let wall_end = now_us();
            log::info!("flac-cost: kind={} phase={} blocks={} bytes={} max_frame_bytes={} encoding_us={} wall_us={} task_runtime_us={} runtime_valid={}",
                kind, phase, blocks, bytes, largest, encoding_us, wall_end - wall_start,
                cpu_end.wrapping_sub(cpu_start), wall_end <= u64::from(u32::MAX));
        }
    }
}

fn now_us() -> u64 {
    // SAFETY: ESP-IDF initializes its monotonic timer before main.
    unsafe { sys::esp_timer_get_time() as u64 }
}

fn runtime_us() -> u32 {
    let mut status = sys::TaskStatus_t::default();
    // SAFETY: NULL selects this live task; no stack scan or task-state lookup is requested.
    unsafe {
        sys::vTaskGetInfo(
            std::ptr::null_mut(),
            &mut status,
            0,
            sys::eTaskState_eRunning,
        );
    }
    status.ulRunTimeCounter
}
