// ABOUTME: Disconnects Wi-Fi once while the real production firmware continues capturing audio.
// ABOUTME: Logs injection results; the host must verify production reconnect and fresh streams.

// Flash with scripts/flash --example network-faults; keep cfg.toml shazam_enabled=false.
// Before injection verify streams advance and record capture counters. Require successful
// DISCONNECT_RESULT, then production wifi: disconnected/reconnected logs, unchanged capture
// fault counters and fresh decodable streams. A fixture marker alone is not a passing test.
// Restore production with scripts/flash afterward. Only the production supervisor reconnects.

use std::time::Duration;

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys;

#[path = "../src/threads.rs"]
mod threads;

fn main() -> anyhow::Result<()> {
    threads::ThreadSpec {
        name: c"network-faults",
        core: Core::Core1,
        priority: 4,
        stack_bytes: 4 * 1024,
    }
    .spawn(|| {
        // Allow startup, source settling and live baseline peers before the one-shot injection.
        std::thread::sleep(Duration::from_secs(60));
        log::info!("network-faults: marker=DISCONNECT_REQUEST ms={}", millis());
        // SAFETY: this public IDF API has no pointer arguments and returns an error if
        // production startup has not initialized/started Wi-Fi. It does not destroy its owner.
        let result = unsafe { sys::esp_wifi_disconnect() };
        if result == sys::ESP_OK {
            log::info!(
                "network-faults: marker=DISCONNECT_RESULT ms={} result={result}",
                millis()
            );
        } else {
            log::error!(
                "network-faults: marker=DISCONNECT_RESULT ms={} result={result}",
                millis()
            );
        }
        // Exit: no forced connect, repeat injection, or additional production state.
    })?;
    esp_shazam_firmware::run()
}

fn millis() -> u64 {
    // SAFETY: ESP-IDF initializes this monotonic timer before main.
    unsafe { sys::esp_timer_get_time() as u64 / 1_000 }
}
