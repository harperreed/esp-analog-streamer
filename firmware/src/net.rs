// ABOUTME: Wi-Fi station with a reconnect supervisor, plus SNTP wall-clock time.
// ABOUTME: ESP-IDF never reconnects by itself, so `supervise` owns the link on the main thread.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::EspSntp;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// Before SNTP sets it, the clock starts at 1970. Anything earlier than 2026-01-01T00:00:00Z means "not synced".
const CLOCK_SANE_AFTER_SECS: u64 = 1_767_225_600;

/// How often `supervise` looks at the link, so a reconnect gets logged within a second.
const LINK_POLL: Duration = Duration::from_secs(1);

/// While the station is off the AP, a new connect attempt starts this often. An attempt scans for
/// about 2.4 s, then joins or gives up, so attempts don't overlap.
const RETRY_INTERVAL: Duration = Duration::from_secs(5);

pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn unix_secs() -> u64 {
    unix_ms() / 1_000
}

/// True once SNTP has set the wall clock; Shazam requests carry a timestamp.
pub fn clock_synced() -> bool {
    unix_secs() > CLOCK_SANE_AFTER_SECS
}

/// Starts Wi-Fi and tries one connect. A failed first connect is logged, not fatal: `supervise` keeps trying.
pub fn start_wifi(
    modem: impl WifiModemPeripheral + 'static,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    ssid: &str,
    pass: &str,
) -> Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid
            .try_into()
            .map_err(|_| anyhow!("wifi_ssid is longer than 32 bytes"))?,
        password: pass
            .try_into()
            .map_err(|_| anyhow!("wifi_pass is longer than 64 bytes"))?,
        auth_method: if pass.is_empty() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        },
        ..Default::default()
    }))?;
    wifi.start()?;
    // Blocks for up to 15 s per step; only boot pays that. `supervise` never waits on a connect.
    match wifi.connect().and_then(|()| wifi.wait_netif_up()) {
        Ok(()) => log_link_up(&wifi, "connected"),
        Err(e) => log::warn!("wifi: connect failed: {e}"),
    }
    Ok(wifi)
}

/// Never returns. Polls the link every second. While the station is off the AP, it starts a
/// non-blocking connect attempt every 5 s, so the board rejoins within one attempt of the AP returning.
pub fn supervise(mut wifi: BlockingWifi<EspWifi<'static>>) -> ! {
    let mut up = link_up(&wifi);
    // Until the first time the link comes up, that event is "connected", not "reconnected".
    let mut ever_up = up;
    let mut last_attempt: Option<Instant> = None;
    loop {
        std::thread::sleep(LINK_POLL);
        if link_up(&wifi) {
            if !up {
                log_link_up(&wifi, if ever_up { "reconnected" } else { "connected" });
                up = true;
                ever_up = true;
            }
            continue;
        }
        if up {
            log::warn!("wifi: disconnected");
            up = false;
        }
        // Joined but without an IP means DHCP is still at work: leave the link alone.
        let joined = wifi.is_connected().unwrap_or(false);
        let attempt_due = last_attempt.is_none_or(|at| at.elapsed() >= RETRY_INTERVAL);
        if !joined && attempt_due {
            last_attempt = Some(Instant::now());
            // Fails when an attempt is still running ("sta is connecting"); that one carries on.
            if let Err(e) = wifi.wifi_mut().connect() {
                log::debug!("wifi: connect attempt not started: {e}");
            }
        }
    }
}

/// Joined to the AP and holding an IP address.
fn link_up(wifi: &BlockingWifi<EspWifi<'static>>) -> bool {
    wifi.is_up().unwrap_or(false)
}

fn log_link_up(wifi: &BlockingWifi<EspWifi<'static>>, event: &str) {
    match wifi.wifi().sta_netif().get_ip_info() {
        Ok(info) => log::info!("wifi: {event}, IP {}", info.ip),
        Err(e) => log::info!("wifi: {event} (IP unknown: {e})"),
    }
}

/// Keep the returned handle alive: dropping it stops SNTP.
pub fn start_sntp() -> Result<EspSntp<'static>> {
    Ok(EspSntp::new_default()?)
}
