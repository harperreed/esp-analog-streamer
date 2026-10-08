// ABOUTME: Suspends the production shared encoder while capture and networking keep running.
// ABOUTME: The host verifies freshness closures, unchanged capture counters and new playback.

// Run scripts/encoder-fault-check after flashing this example with Shazam off.
// Encoder and observer are pinned to core1. A brief scheduler suspension makes
// the blocked-state check and target suspension indivisible on that core, even
// under priority inheritance. No logs, allocation or waits occur in that section.
// This depends on the production task name, affinity, priority and perpetual lifetime.
// Host counters still reject any capture loss; markers alone do not establish success.
// Seventeen seconds exceeds 12.032s history, 2s freshness and 2.5s close slack.
// Always restore production after this separate hardware fixture.

use esp_idf_svc::{hal::cpu::Core, sys};
use std::time::{Duration, Instant};
#[path = "../src/threads.rs"]
mod threads;

struct Suspended(sys::TaskHandle_t);
impl Drop for Suspended {
    fn drop(&mut self) {
        // SAFETY: production never deletes/replaces this checked encoder task.
        unsafe { sys::vTaskResume(self.0) };
    }
}
fn observe() -> anyhow::Result<()> {
    std::thread::sleep(Duration::from_secs(30));
    // SAFETY: static task name; production's shared encoder is perpetual.
    let encoder = unsafe { sys::xTaskGetHandle(c"stream-encode".as_ptr()) };
    anyhow::ensure!(!encoder.is_null(), "encoder task missing");
    let arm_ms = millis();
    // SAFETY: logger uses this same RTOS timestamp source in the pinned config.
    let arm_rtos_ms = unsafe { sys::esp_log_timestamp() };
    log::info!("encoder-faults: marker=ARM ms={arm_ms} rtos_ms={arm_rtos_ms}");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // SAFETY: pinned IDF tasks.c suspends this core's scheduler. The target is
        // pinned here too, so it cannot become running between these operations.
        // vTaskSuspend of a nonrunning target only removes list entries: its yield
        // branches apply to running/self targets, which this blocked check excludes.
        let blocked = unsafe {
            sys::vTaskSuspendAll();
            let blocked = sys::eTaskGetState(encoder) == sys::eTaskState_eBlocked;
            if blocked {
                sys::vTaskSuspend(encoder);
            }
            sys::xTaskResumeAll();
            blocked
        };
        if blocked {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "encoder did not enter a blocked wait"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let suspended = Suspended(encoder);
    let at = millis();
    std::thread::sleep(Duration::from_secs(17));
    drop(suspended);
    let resumed = millis();
    // Logging can block on a FILE/UART lock owned by the target. Never log while it
    // is suspended: these saved event times intentionally arrive after resumption.
    log::info!("encoder-faults: marker=SUSPEND ms={at} hold_ms=17000 deferred=true");
    log::info!("encoder-faults: marker=RESUME ms={resumed} deferred=true");
    Ok(())
}
fn main() -> anyhow::Result<()> {
    threads::ThreadSpec {
        name: c"encoder-faults",
        core: Core::Core1,
        priority: 4,
        stack_bytes: 4 * 1024,
    }
    .spawn(|| {
        if let Err(error) = observe() {
            log::error!(
                "encoder-faults: marker=FAIL ms={} error={error:#}",
                millis()
            );
        }
    })?;
    esp_shazam_firmware::run()
}
fn millis() -> u64 {
    // SAFETY: IDF initializes the monotonic timer before entering main.
    unsafe { sys::esp_timer_get_time() as u64 / 1_000 }
}
