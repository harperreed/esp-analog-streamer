// ABOUTME: Measures bounded standalone quantization and unchanged FLAC encoder costs on core 1.
// ABOUTME: Reports deterministic receipts, memory endpoints and timing without capture or networking.

use esp_idf_svc::{hal::cpu::Core, sys};
use esp_shazam_core::flac::{Encoder, BLOCK_FRAMES, CHANNELS, MAX_FRAME_BYTES};
use std::time::Duration;

#[path = "quant-cost/contract.rs"]
mod contract;
#[path = "../src/threads.rs"]
mod threads;
use contract::{Receipt, BLOCKS, CHUNK_SAMPLES, EXPECTED, KINDS, ORDERS, SEEDS};

fn main() -> anyhow::Result<()> {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let (finished, result) = std::sync::mpsc::channel();
    threads::ThreadSpec {
        name: c"quant-cost",
        core: Core::Core1,
        priority: 7,
        stack_bytes: 4 * 1024,
    }
    .spawn(move || {
        benchmark();
        finished.send(()).expect("probe receiver");
    })?;
    result.recv()?;
    log::info!("quant-cost: complete phases=180 assertions_passed=true capture=false wifi=false audio=false");
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn benchmark() {
    let mut encoder = Encoder::new();
    let mut input = vec![0_i32; BLOCK_FRAMES * CHANNELS];
    let mut chunk = vec![0_i32; CHUNK_SAMPLES];
    let mut output = Vec::<u8>::with_capacity(MAX_FRAME_BYTES);
    let output_ptr = output.as_ptr();
    let output_capacity = output.capacity();
    // SAFETY: IDF address predicates inspect only the live allocations' addresses.
    let (input_psram, chunk_psram, output_psram) = unsafe {
        (
            sys::esp_ptr_external_ram(input.as_ptr().cast()),
            sys::esp_ptr_external_ram(chunk.as_ptr().cast()),
            sys::esp_ptr_external_ram(output.as_ptr().cast()),
        )
    };
    assert!(input_psram, "source buffer must be in PSRAM");
    assert!(!chunk_psram, "480-i32 chunk must be in internal RAM");
    assert!(output_capacity >= MAX_FRAME_BYTES);
    let mut empty_pair_sum = 0;
    let mut empty_pair_max = 0;
    for _ in 0..256 {
        let started = now_us();
        let elapsed = now_us() - started;
        empty_pair_sum += elapsed;
        empty_pair_max = empty_pair_max.max(elapsed);
    }
    log::info!("quant-cost: setup input_bytes={} chunk_bytes={} rng_state_bytes={} input_psram={} chunk_psram={} output_psram={} output_capacity={} measured_blocks={} orders=6 header_number=0 timer_calls_per_block=78 empty_timer_pairs=256 empty_timer_pair_sum_us={} empty_timer_pair_max_us={} redundant_requantization=true approximate_prng=true",
        input.len() * 4, chunk.len() * 4, std::mem::size_of::<[u32; 2]>(), input_psram,
        chunk_psram, output_psram, output_capacity, BLOCKS, empty_pair_sum, empty_pair_max);
    for (kind_index, kind) in KINDS.into_iter().enumerate() {
        contract::fill_input(kind, &mut input);
        let source_hash = contract::source_hash(&input);
        assert_eq!(
            source_hash,
            contract::SOURCE_HASHES[kind_index],
            "host source mismatch"
        );
        for (order_index, order) in ORDERS.into_iter().enumerate() {
            for policy in order {
                for (phase_index, (phase, blocks)) in [("warmup", 1), ("measured", BLOCKS)]
                    .into_iter()
                    .enumerate()
                {
                    let mut states = SEEDS;
                    let mut receipt = Receipt::new();
                    // Match flac-cost: commit prior setup/logging runtime before sampling.
                    runtime_yield();
                    let heap_before = free_heap();
                    let cpu_before = runtime_us();
                    let wall_before = now_us();
                    let mut quant_us = 0;
                    let mut encoder_us = 0;
                    let mut block_us = 0;
                    let mut max_quant_us = 0;
                    let mut max_encoder_us = 0;
                    let mut max_block_us = 0;
                    for _ in 0..blocks {
                        let block_start = now_us();
                        let mut block_quant = 0;
                        let started = now_us();
                        encoder.reset();
                        let mut block_encoder = now_us() - started;
                        for samples in input.chunks(CHUNK_SAMPLES) {
                            let started = now_us();
                            contract::transform(
                                samples,
                                &mut chunk[..samples.len()],
                                policy,
                                &mut states,
                            );
                            block_quant += now_us() - started;
                            let started = now_us();
                            encoder.push(&chunk[..samples.len()]);
                            block_encoder += now_us() - started;
                            // Checksum is outside both quantization and encoder timers.
                            receipt.add_pcm(&chunk[..samples.len()]);
                        }
                        let started = now_us();
                        encoder.encode_block(0, &mut output);
                        block_encoder += now_us() - started;
                        assert!(output.len() <= MAX_FRAME_BYTES);
                        assert_eq!(output.as_ptr(), output_ptr, "output moved");
                        assert_eq!(output.capacity(), output_capacity, "output grew");
                        receipt.add(&output);
                        let block_elapsed = now_us() - block_start;
                        quant_us += block_quant;
                        encoder_us += block_encoder;
                        block_us += block_elapsed;
                        max_quant_us = max_quant_us.max(block_quant);
                        max_encoder_us = max_encoder_us.max(block_encoder);
                        max_block_us = max_block_us.max(block_elapsed);
                        // Matches flac-cost boundary; wall includes sleep, task runtime excludes it.
                        runtime_yield();
                    }
                    let wall_after = now_us();
                    let cpu_after = runtime_us();
                    let heap_after = free_heap();
                    // SAFETY: NULL selects this live task; IDF watermark is in bytes.
                    let stack_free =
                        unsafe { sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
                    let expected = EXPECTED[kind_index][policy.index()][phase_index];
                    assert_eq!(receipt, expected.receipt, "host output receipt mismatch");
                    assert_eq!(states, expected.states, "host RNG state mismatch");
                    assert_eq!(heap_before, heap_after, "phase heap changed");
                    log::info!("quant-cost: kind={} order={} policy={} phase={} blocks={} source_hash={} bytes={} frame_hash={} pcm_hash={} max_frame_bytes={} rng_left={} rng_right={} quant_us={} encoder_us={} block_wall_us={} max_quant_us={} max_encoder_us={} max_block_wall_us={} wall_us={} task_runtime_us={} runtime_valid={} stack_free_bytes={} heap_internal_before={} heap_internal_after={} heap_external_before={} heap_external_after={} output_ptr={:p} output_capacity={} output_storage_reused=true host_receipt_match=true",
                        kind, order_index, policy.name(), phase, blocks, source_hash, receipt.bytes, receipt.hash,
                        receipt.pcm_hash, receipt.largest, states[0], states[1], quant_us, encoder_us, block_us,
                        max_quant_us, max_encoder_us, max_block_us, wall_after - wall_before,
                        cpu_after.wrapping_sub(cpu_before), wall_after <= u64::from(u32::MAX), stack_free,
                        heap_before.0, heap_after.0, heap_before.1, heap_after.1, output_ptr, output_capacity);
                }
            }
        }
    }
}

fn runtime_yield() {
    std::thread::sleep(Duration::from_millis(10));
}

fn now_us() -> u64 {
    // SAFETY: IDF initialized its monotonic timer before main.
    unsafe { sys::esp_timer_get_time() as u64 }
}

fn runtime_us() -> u32 {
    let mut status = sys::TaskStatus_t::default();
    // SAFETY: NULL selects this task; no stack scan or state lookup is requested.
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

fn free_heap() -> (usize, usize) {
    // SAFETY: IDF heap queries read initialized allocator accounting.
    unsafe {
        (
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT),
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT),
        )
    }
}
