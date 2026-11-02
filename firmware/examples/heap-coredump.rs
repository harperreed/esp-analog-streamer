// ABOUTME: Measures bounded pinned-IDF heap tracing with a standalone UART crash fixture.
// ABOUTME: Starts no application services and leaves normal-config runs alive without a crash.

use esp_idf_svc::sys;
use std::ffi::c_void;
use std::ptr;

const INTERNAL: u32 = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
const EXTERNAL: u32 = sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT;
const BLOCK_BYTES: usize = 128;
const ROUNDS: usize = 500;

// Device assertions are the behavioral acceptance: known live/freed records,
// no overflow, a PSRAM checksum, and a decoded named abort frame. Host tests
// cannot establish allocator hooks, generated ABI or panic-path behavior.
#[cfg(esp_idf_heap_tracing_standalone)]
mod trace {
    use super::*;
    use sys::diagnostics as api;

    const RECORDS: usize = 64;
    // Static storage is internal .bss; no large stack temporary or PSRAM fallback.
    static mut BUFFER: std::mem::MaybeUninit<[api::heap_trace_record_t; RECORDS]> =
        std::mem::MaybeUninit::zeroed();

    pub fn initialize() {
        assert!(cfg!(esp_idf_esp_coredump_enable_to_uart));
        assert!(!cfg!(esp_idf_esp_coredump_capture_dram));
        assert_eq!(std::mem::size_of::<api::heap_trace_record_t>(), 40);
        // SAFETY: this fixture alone owns the static buffer for its entire lifetime.
        let buffer = ptr::addr_of_mut!(BUFFER).cast::<api::heap_trace_record_t>();
        unsafe {
            assert!(!sys::esp_ptr_external_ram(buffer.cast()));
            assert_eq!(
                api::heap_trace_init_standalone(buffer, RECORDS),
                sys::ESP_OK
            );
        }
        log::info!("DIAG_BUFFER records={} bytes={}", RECORDS, RECORDS * 40);
    }

    pub fn start(leaks: bool) {
        let mode = if leaks {
            api::heap_trace_mode_t_HEAP_TRACE_LEAKS
        } else {
            api::heap_trace_mode_t_HEAP_TRACE_ALL
        };
        // SAFETY: initialize installed a stable live buffer; calls are sequential.
        unsafe { assert_eq!(api::heap_trace_start(mode), sys::ESP_OK) };
    }

    pub fn stop(label: &str) -> api::heap_trace_summary_t {
        // SAFETY: tracing is active; the generated POD summary has no invalid zero state.
        let summary = unsafe {
            assert_eq!(api::heap_trace_stop(), sys::ESP_OK);
            let mut summary: api::heap_trace_summary_t = std::mem::zeroed();
            assert_eq!(api::heap_trace_summary(&mut summary), sys::ESP_OK);
            summary
        };
        assert_eq!(summary.has_overflowed, 0, "trace overflow");
        log::info!(
            "DIAG_TRACE phase={} allocations={} frees={} records={} high_water={} capacity={} overflow={}",
            label, summary.total_allocations, summary.total_frees, summary.count,
            summary.high_water_mark, summary.capacity, summary.has_overflowed
        );
        summary
    }

    pub fn known_allocations() {
        start(false);
        let live = allocate(INTERNAL);
        let released = allocate(INTERNAL);
        // SAFETY: released is uniquely owned and is not dereferenced after freeing.
        unsafe { sys::heap_caps_free(released) };
        let summary = stop("known");
        let mut live_found = false;
        let mut freed_found = false;
        for index in 0..summary.count {
            // SAFETY: tracing is stopped; index is within the captured record count.
            let record = unsafe {
                let mut record: api::heap_trace_record_t = std::mem::zeroed();
                assert_eq!(api::heap_trace_get(index, &mut record), sys::ESP_OK);
                record
            };
            live_found |= record.address == live && record.size == BLOCK_BYTES && !record.freed;
            freed_found |= record.address == released && record.size == BLOCK_BYTES && record.freed;
        }
        // Free before asserting so failures cannot leave the intentional owner live.
        unsafe { sys::heap_caps_free(live) };
        assert!(live_found && freed_found, "known allocation trace mismatch");
        log::info!("DIAG_KNOWN live=1 freed=1 passed=1");
    }
}

fn allocate(caps: u32) -> *mut c_void {
    // SAFETY: fixed positive size and supported capability mask; null is checked.
    let block = unsafe { sys::heap_caps_malloc(BLOCK_BYTES, caps) };
    assert!(!block.is_null(), "fixture allocation failed");
    block
}

fn now() -> i64 {
    // SAFETY: monotonic getter has no caller preconditions.
    unsafe { sys::esp_timer_get_time() }
}

fn snapshot(label: &str) {
    for (kind, caps) in [("internal", INTERNAL), ("psram", EXTERNAL)] {
        // SAFETY: read-only allocator/task statistics; NULL selects the current task.
        unsafe {
            log::info!(
                "DIAG_HEAP phase={} kind={} free={} largest={} minimum={} stack_free={}",
                label,
                kind,
                sys::heap_caps_get_free_size(caps),
                sys::heap_caps_get_largest_free_block(caps),
                sys::heap_caps_get_minimum_free_size(caps),
                sys::uxTaskGetStackHighWaterMark(ptr::null_mut())
            );
        }
    }
}

fn runtime() -> u32 {
    // SAFETY: generated POD is initialized by vTaskGetInfo; NULL selects this task.
    unsafe {
        let mut status: sys::TaskStatus_t = std::mem::zeroed();
        sys::vTaskGetInfo(ptr::null_mut(), &mut status, 0, sys::eTaskState_eInvalid);
        status.ulRunTimeCounter
    }
}

fn allocation_loop(label: &str, enabled: bool) {
    snapshot(label);
    #[cfg(esp_idf_heap_tracing_standalone)]
    if enabled {
        // LEAKS removes freed records, so repeated operations do not fill the buffer.
        trace::start(true);
    }
    let before_runtime = runtime();
    let before = now();
    let mut max_batch_us = 0;
    let mut max_yield_us = 0;
    for _ in 0..ROUNDS {
        let batch_start = now();
        for _ in 0..16 {
            let block = allocate(INTERNAL);
            // SAFETY: block is a live unique allocation of BLOCK_BYTES bytes.
            unsafe {
                ptr::write_bytes(block.cast::<u8>(), 0x5a, BLOCK_BYTES);
                std::hint::black_box(ptr::read_volatile(block.cast::<u8>()));
                sys::heap_caps_free(block);
            }
        }
        max_batch_us = max_batch_us.max(now() - batch_start);
        let yield_start = now();
        // SAFETY: one actual RTOS tick releases the core; no sub-tick busy wait.
        unsafe { sys::vTaskDelay(1) };
        max_yield_us = max_yield_us.max(now() - yield_start);
    }
    let elapsed = now() - before;
    let runtime_us = runtime().wrapping_sub(before_runtime);
    #[cfg(esp_idf_heap_tracing_standalone)]
    if enabled {
        let summary = trace::stop(label);
        assert!(summary.total_allocations >= ROUNDS * 16);
        assert!(summary.total_frees >= ROUNDS * 16);
    }
    log::info!(
        "DIAG_LOOP phase={} tracing={} operations={} elapsed_us={} runtime_us={} max_batch_us={} max_yield_us={}",
        label, enabled && cfg!(esp_idf_heap_tracing_standalone), ROUNDS * 16,
        elapsed, runtime_us, max_batch_us, max_yield_us
    );
    snapshot(label);
}

#[cfg(esp_idf_heap_tracing_standalone)]
#[inline(never)]
fn intentional_fixture_abort() -> ! {
    // SAFETY: static NUL-terminated reason remains live throughout the panic path.
    unsafe { sys::esp_system_abort(c"heap-coredump controlled fixture abort".as_ptr()) }
}

fn main() {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    assert_eq!(
        cfg!(esp_idf_heap_tracing_standalone),
        cfg!(esp_idf_esp_coredump_enable_to_uart)
    );
    log::info!(
        "DIAG_BEGIN tracing={} rounds={} tick_hz={}",
        cfg!(esp_idf_heap_tracing_standalone),
        ROUNDS,
        sys::CONFIG_FREERTOS_HZ
    );
    snapshot("boot");
    let sentinel = allocate(EXTERNAL);
    // SAFETY: sentinel is a uniquely owned BLOCK_BYTES-byte external allocation.
    let checksum = unsafe {
        assert!(sys::esp_ptr_external_ram(sentinel));
        ptr::write_bytes(sentinel.cast::<u8>(), 0xa5, BLOCK_BYTES);
        let checksum: u32 = (0..BLOCK_BYTES)
            .map(|index| u32::from(ptr::read_volatile(sentinel.cast::<u8>().add(index))))
            .sum();
        sys::heap_caps_free(sentinel);
        checksum
    };
    assert_eq!(checksum, BLOCK_BYTES as u32 * 0xa5);
    log::info!(
        "DIAG_PSRAM bytes={} checksum={} passed=1",
        BLOCK_BYTES,
        checksum
    );
    #[cfg(esp_idf_heap_tracing_standalone)]
    {
        trace::initialize();
        trace::known_allocations();
    }
    for (label, enabled) in [("off1", false), ("on", true), ("off2", false)] {
        allocation_loop(label, enabled);
    }
    log::info!("DIAG_COMPLETE passed=1");
    #[cfg(esp_idf_heap_tracing_standalone)]
    {
        // Give the UART collector time to receive the acceptance receipt before panic.
        unsafe { sys::vTaskDelay(sys::CONFIG_FREERTOS_HZ) };
        intentional_fixture_abort();
    }
    #[cfg(not(esp_idf_heap_tracing_standalone))]
    {
        log::info!("CONTROL_COMPLETE passed=1");
        loop {
            unsafe { sys::vTaskDelay(sys::CONFIG_FREERTOS_HZ) };
        }
    }
}
