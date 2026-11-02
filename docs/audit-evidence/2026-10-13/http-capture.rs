// ABOUTME: Exercises three read-only Sonos HTTP client lifetimes during production capture.
// ABOUTME: Emits bounded phase and heap receipts without logging peer identities or media URIs.

// Throwaway diagnostic: build with SONOS_CAPTURE_GROUP containing a fresh discovered
// DiscoveryGroup JSON object. Keep recognition/autoplay off and the speaker stopped.
// Start two real MP3 debug listeners after BASELINE; retain decoded soak and UART evidence.
// These markers alone prove neither audio continuity nor controller recovery policy.

use std::time::{Duration, Instant};

use esp_idf_svc::{hal::cpu::Core, sys};
use esp_shazam_core::{
    sonos::DiscoveryGroup,
    sonos_upnp::{Client, TransportState},
};

#[allow(dead_code)]
#[path = "../src/config.rs"]
mod config;
#[path = "../src/threads.rs"]
mod threads;

const _: () = assert!(
    !config::CONFIG.shazam_enabled,
    "http-capture requires shazam_enabled=false"
);
const TIMEOUT: Duration = Duration::from_secs(5);

fn selected_group() -> anyhow::Result<DiscoveryGroup> {
    let json = option_env!("SONOS_CAPTURE_GROUP")
        .ok_or_else(|| anyhow::anyhow!("SONOS_CAPTURE_GROUP is required at build time"))?;
    // Do not return parser details: they can contain private input text.
    let group: DiscoveryGroup = serde_json::from_str(json)
        .map_err(|_| anyhow::anyhow!("SONOS_CAPTURE_GROUP is invalid JSON"))?;
    anyhow::ensure!(
        group.is_leader
            && !group.player_id.trim().is_empty()
            && !group.group_id.trim().is_empty()
            && !group.household_id.trim().is_empty()
            && !group.host.is_unspecified()
            && !group.host.is_loopback()
            && !group.host.is_multicast()
            && !group.host.is_broadcast()
            && group.host.octets()[0] != 0
            && group.host.octets()[0] < 224,
        "SONOS_CAPTURE_GROUP requires a leader, nonempty IDs and a unicast IPv4 host"
    );
    Ok(group)
}

fn marker(name: &str, cycle: u32) {
    let caps = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
    // SAFETY: timer and heap queries are read-only ESP-IDF APIs initialized before main.
    let (ms, free, largest, stack_free_bytes) = unsafe {
        (
            sys::esp_timer_get_time() / 1000,
            sys::heap_caps_get_free_size(caps),
            sys::heap_caps_get_largest_free_block(caps),
            // Current task only; pinned ESP32 StackType_t is u8.
            sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()),
        )
    };
    log::info!(
        "http-capture: marker={name} cycle={cycle} ms={ms} internal_free={free} internal_largest={largest} stack_free_bytes={stack_free_bytes}"
    );
}

fn experiment(group: &DiscoveryGroup) -> Result<(), (u32, u32)> {
    for cycle in 1..=3 {
        marker("BEGIN", cycle);
        let client = Client::connect(group, Instant::now() + TIMEOUT).map_err(|_| (cycle, 1))?;
        marker("CONNECTED", cycle);
        // Both reads share one fresh absolute deadline. observe verifies identity again.
        let deadline = Instant::now() + TIMEOUT;
        let result = (|| {
            let uri = client.media_uri(deadline).map_err(|_| (cycle, 2))?;
            let state = client
                .observe(&uri, deadline, || Ok(()))
                .map_err(|_| (cycle, 3))?;
            if state != Some(TransportState::Stopped) {
                return Err((cycle, 4));
            }
            log::info!("http-capture: cycle={cycle} transport=STOPPED");
            marker("OBSERVED", cycle);
            Ok(())
        })();
        drop(client);
        marker("DROPPED", cycle);
        result?;
        if cycle < 3 {
            std::thread::sleep(Duration::from_secs(5));
        }
    }
    std::thread::sleep(Duration::from_secs(20));
    marker("RECOVERED", 3);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    // Reject absent/invalid input before starting production or a diagnostic task.
    let group = selected_group()?;
    threads::ThreadSpec {
        name: c"http-capture",
        core: Core::Core0,
        priority: 2,
        stack_bytes: 16 * 1024,
    }
    .spawn(move || {
        std::thread::sleep(Duration::from_secs(15));
        marker("BASELINE", 0);
        std::thread::sleep(Duration::from_secs(25));
        if let Err((cycle, stage)) = experiment(&group) {
            marker("FAIL", cycle);
            log::error!("http-capture: cycle={cycle} stage={stage}");
        }
    })?;
    esp_shazam_firmware::run()
}
