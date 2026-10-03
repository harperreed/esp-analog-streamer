// ABOUTME: Wi-Fi station with a reconnect supervisor, plus SNTP wall-clock time.
// ABOUTME: ESP-IDF never reconnects by itself, so `supervise` owns the link on the main thread.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::EspSntp;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// Before SNTP sets it, the clock starts at 1970. Anything earlier than 2026-01-01T00:00:00Z means "not synced".
const CLOCK_SANE_AFTER_SECS: u64 = 1_767_225_600;

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
    connect(&mut wifi);
    Ok(wifi)
}

fn connect(wifi: &mut BlockingWifi<EspWifi<'static>>) -> bool {
    match wifi.connect().and_then(|()| wifi.wait_netif_up()) {
        Ok(()) => {
            match wifi.wifi().sta_netif().get_ip_info() {
                Ok(info) => log::info!("wifi: connected, IP {}", info.ip),
                Err(e) => log::info!("wifi: connected (IP unknown: {e})"),
            }
            true
        }
        Err(e) => {
            log::warn!("wifi: connect failed: {e}");
            false
        }
    }
}

/// Never returns. Checks the link every 5 s and reconnects when it has dropped.
pub fn supervise(mut wifi: BlockingWifi<EspWifi<'static>>) -> ! {
    let mut was_up = wifi.is_connected().unwrap_or(false);
    loop {
        std::thread::sleep(Duration::from_secs(5));
        if wifi.is_connected().unwrap_or(false) {
            was_up = true;
            continue;
        }
        if was_up {
            log::warn!("wifi: disconnected");
            was_up = false;
        }
        if connect(&mut wifi) {
            log::info!("wifi: reconnected");
            was_up = true;
        }
    }
}

/// Keep the returned handle alive: dropping it stops SNTP.
pub fn start_sntp() -> Result<EspSntp<'static>> {
    Ok(EspSntp::new_default()?)
}
