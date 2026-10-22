// ABOUTME: Suspends the real capture task to verify short-pause, restart, and reboot recovery.
// ABOUTME: Runs the production startup and emits ordered UART markers for the host to verify.

// Only this hardware example injects faults; production has no injection endpoint or mode.
// The host must observe SHORT_SUSPEND/RESUME, RECOVERY_SUSPEND/RESUME, FINAL_SUSPEND,
// the asynchronous restart report, actual software reset, and ESP_RST_SW plus POST_REBOOT.
// Recovery must not wait for a reboot log: the UART logger can itself be blocked.
// POST_REBOOT alone proves nothing: any software reset skips injection on the next boot.
// Suspending at an arbitrary instruction can also freeze a held mutex, so host checks must
// distinguish freshness timeout, reset invalidation, and blocked-lock recovery from logs.

use std::time::Duration;

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys;

use esp_shazam_firmware::threads;

fn main() -> anyhow::Result<()> {
    // SAFETY: ESP-IDF initializes reset information before entering main.
    let reason = unsafe { sys::esp_reset_reason() };
    threads::ThreadSpec {
        name: c"capture-faults",
        core: Core::Core1,
        priority: 7,
        stack_bytes: 4 * 1024,
    }
    .spawn(move || observe(reason))?;
    esp_shazam_firmware::run()
}

fn observe(reason: sys::esp_reset_reason_t) {
    // Production run initializes logging and starts capture/network during this baseline grace.
    std::thread::sleep(Duration::from_secs(15));
    log::info!(
        "capture-faults: marker=BOOT ms={} reset_reason={reason}",
        millis()
    );
    if reason == sys::esp_reset_reason_t_ESP_RST_SW {
        marker("POST_REBOOT");
        return;
    }
    // SAFETY: this is a nul-terminated static name. The production audio task is perpetual:
    // I2S restart happens within that owner task, without deleting/replacing its task handle.
    let audio = unsafe { sys::xTaskGetHandle(c"audio".as_ptr()) };
    if audio.is_null() {
        marker("FAIL_AUDIO_TASK_MISSING");
        return;
    }
    marker("AUDIO_TASK_FOUND");

    suspend(audio, "SHORT_SUSPEND");
    // Below the 45 ms DMA queue; scheduler jitter or existing lag can consume the remaining
    // headroom. The host must verify actual continuity/overflow counts instead of assuming it.
    std::thread::sleep(Duration::from_millis(20));
    resume(audio, "SHORT_RESUME");
    std::thread::sleep(Duration::from_secs(10));

    suspend(audio, "RECOVERY_SUSPEND");
    std::thread::sleep(Duration::from_millis(3_500));
    resume(audio, "RECOVERY_RESUME");
    std::thread::sleep(Duration::from_secs(10));

    suspend(audio, "FINAL_SUSPEND");
    // No fixture restart or panic: the independent 3 s stall + 3 s recovery supervisor must reboot.
    std::thread::sleep(Duration::from_secs(10));
    marker("FAIL_SUPERVISOR_DID_NOT_REBOOT");
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn suspend(audio: sys::TaskHandle_t, name: &str) {
    marker(name);
    // SAFETY: audio was checked nonnull and belongs to the perpetual production capture task.
    // The observer is a different task and resumes the first two intentional suspensions.
    unsafe { sys::vTaskSuspend(audio) };
}

fn resume(audio: sys::TaskHandle_t, name: &str) {
    // SAFETY: the perpetual capture task remains live while suspended; no task handle is freed.
    unsafe { sys::vTaskResume(audio) };
    marker(name);
}

fn marker(name: &str) {
    log::info!("capture-faults: marker={name} ms={}", millis());
}

fn millis() -> u64 {
    // SAFETY: ESP-IDF initializes this monotonic timer before main.
    unsafe { sys::esp_timer_get_time() as u64 / 1_000 }
}
