// ABOUTME: Wi-Fi station with a reconnect supervisor, SNTP wall-clock time, and the mDNS name.
// ABOUTME: ESP-IDF never reconnects by itself, so `supervise` owns the link on the main thread.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use esp_idf_svc::eventloop::{EspSubscription, EspSystemEventLoop, System};
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::mdns::EspMdns;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::EspSntp;
use esp_idf_svc::sys::{self, esp};
use esp_idf_svc::wifi::{
    AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi, WifiEvent,
};
use esp_shazam_core::network_lifecycle::{Network, Snapshot};

#[path = "net_policy.rs"]
mod policy;
use policy::RadioPolicy;

#[path = "network_observation.rs"]
mod observation;
static DRIVER_DISCONNECT_COUNT: observation::DisconnectCount = observation::DisconnectCount::new();

/// Raw wrapping count of driver disconnect events since boot.
pub fn disconnect_count() -> u32 {
    DRIVER_DISCONNECT_COUNT.get()
}

const STREAMING_RADIO: RadioPolicy = RadioPolicy {
    protocol: (sys::WIFI_PROTOCOL_11B | sys::WIFI_PROTOCOL_11G | sys::WIFI_PROTOCOL_11N) as u8,
    power_save: sys::wifi_ps_type_t_WIFI_PS_NONE,
};

/// Holds the disconnect observer for the same lifetime as the station driver.
pub struct WifiStation {
    wifi: BlockingWifi<EspWifi<'static>>,
    network: Network,
    _events: EspSubscription<'static, System>,
}

impl WifiStation {
    pub fn network(&self) -> Network {
        self.network.clone()
    }
}

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
) -> Result<WifiStation> {
    #[cfg(feature = "startup-capture-trace")]
    crate::startup_capture_trace::phase(7, 0);
    let driver = EspWifi::new(modem, sysloop.clone(), Some(nvs));
    #[cfg(feature = "startup-capture-trace")]
    crate::startup_capture_trace::phase(8, driver.as_ref().err().map_or(0, |e| e.code()));
    let mut wifi = BlockingWifi::wrap(driver?, sysloop.clone())?;
    let network = Network::new();
    let observed_network = network.clone();
    let events = sysloop.subscribe::<WifiEvent, _>(move |event| {
        if matches!(
            event,
            WifiEvent::StaConnected(_) | WifiEvent::StaDisconnected(_) | WifiEvent::StaStopped
        ) {
            let before = observed_network.snapshot();
            let after = observed_network.invalidate();
            log_network_change(before, after);
        }
        if let WifiEvent::StaDisconnected(info) = event {
            DRIVER_DISCONNECT_COUNT.observe();
            // Event Debug includes the SSID: report only the reason and signal strength.
            log::warn!(
                "wifi: station disconnected, reason={} rssi={}dBm",
                info.reason(),
                info.rssi()
            );
        }
    })?;
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
    #[cfg(feature = "startup-capture-trace")]
    crate::startup_capture_trace::phase(9, 0);
    let started = wifi.start();
    #[cfg(feature = "startup-capture-trace")]
    crate::startup_capture_trace::phase(10, started.as_ref().err().map_or(0, |e| e.code()));
    started?;
    // A prior diagnostic bitmap can survive reflashing. Apply b/g/n after driver startup,
    // before the first association, rather than relying on the stored/default policy.
    // SAFETY: the station driver is initialized and started; the bitmap uses IDF constants.
    esp!(unsafe {
        sys::esp_wifi_set_protocol(sys::wifi_interface_t_WIFI_IF_STA, STREAMING_RADIO.protocol)
    })?;
    // Modem sleep, ESP-IDF's default, lets the AP hold the board's incoming frames until its next
    // beacon (every 102.4 ms on Harper's AP), so TCP ACKs wait up to that long. At lwIP's
    // 5,760-byte send buffer the LINE IN stream needs round trips under 20 ms (SDD Ruling R27).
    // The board runs on USB power.
    // SAFETY: a plain call with an enum value; EspWifi::new initialized the driver.
    esp!(unsafe { sys::esp_wifi_set_ps(STREAMING_RADIO.power_save) })?;
    log_radio_policy("started")?;
    // Blocks for up to 15 s per step; only boot pays that. `supervise` never waits on a connect.
    match wifi.connect().and_then(|()| wifi.wait_netif_up()) {
        Ok(()) => log_link_up(&wifi, "connected"),
        Err(e) => log::warn!("wifi: connect failed: {e}"),
    }
    observe_link(&wifi, &network);
    Ok(WifiStation {
        wifi,
        network,
        _events: events,
    })
}

/// Never returns. Polls the link every second. While the station is off the AP, it starts a
/// non-blocking connect attempt every 5 s, so the board rejoins within one attempt of the AP returning.
pub fn supervise(station: WifiStation) -> ! {
    let WifiStation {
        mut wifi,
        network,
        _events,
    } = station;
    let mut up = link_up(&wifi);
    // Until the first time the link comes up, that event is "connected", not "reconnected".
    let mut ever_up = up;
    let mut last_attempt: Option<Instant> = None;
    loop {
        std::thread::sleep(LINK_POLL);
        let snapshot = observe_link(&wifi, &network);
        if snapshot.address.is_some() {
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

/// Capture the generation before querying IDF so a disconnect during the query wins.
fn observe_link(wifi: &BlockingWifi<EspWifi<'static>>, network: &Network) -> Snapshot {
    let expected = network.snapshot();
    let address = if link_up(wifi) {
        wifi.wifi()
            .sta_netif()
            .get_ip_info()
            .ok()
            .map(|info| Ipv4Addr::from(info.ip.octets()))
    } else {
        None
    };
    let observed = network.observe(expected, address);
    log_network_change(expected, observed);
    observed
}

fn log_network_change(before: Snapshot, after: Snapshot) {
    if before != after {
        match after.address {
            Some(address) => log::info!(
                "wifi: network generation={} address={address}",
                after.generation
            ),
            None => log::info!("wifi: network generation={} unavailable", after.generation),
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
    if let Err(e) = log_radio_policy(event) {
        log::error!("wifi: {event} radio policy verification failed: {e}");
    }
    let mut ap = sys::wifi_ap_record_t::default();
    // SAFETY: the driver remains owned by wifi; ap is a writable record of the pinned layout.
    match esp!(unsafe { sys::esp_wifi_sta_get_ap_info(&mut ap) }) {
        Ok(()) => log::info!(
            "wifi: {event}, association channel={} rssi={}dBm",
            ap.primary,
            ap.rssi
        ),
        Err(e) => log::warn!("wifi: {event} association readback failed: {e}"),
    }
    let mut phy = 0;
    // SAFETY: the driver is started and phy is writable; disconnect races return an IDF error.
    match esp!(unsafe { sys::esp_wifi_sta_get_negotiated_phymode(&mut phy) }) {
        Ok(()) => log::info!("wifi: {event}, negotiated_phy={phy}"),
        Err(e) => log::warn!("wifi: {event} negotiated PHY readback failed: {e}"),
    }
}

/// Read effective settings independently of the setter results, including after reconnect.
fn log_radio_policy(event: &str) -> Result<()> {
    let mut effective = RadioPolicy {
        protocol: 0,
        power_save: 0,
    };
    // SAFETY: the driver is initialized; both output pointers refer to writable IDF-sized values.
    esp!(unsafe {
        sys::esp_wifi_get_protocol(sys::wifi_interface_t_WIFI_IF_STA, &mut effective.protocol)
    })?;
    esp!(unsafe { sys::esp_wifi_get_ps(&mut effective.power_save) })?;
    log::info!(
        "wifi: {event}, effective protocol=0x{:02x} power_save={}",
        effective.protocol,
        effective.power_save
    );
    STREAMING_RADIO.verify(effective).map_err(|mismatch| {
        anyhow!(
            "expected protocol=0x{:02x} power_save={}, read protocol=0x{:02x} power_save={}",
            mismatch.expected.protocol,
            mismatch.expected.power_save,
            mismatch.effective.protocol,
            mismatch.effective.power_save
        )
    })
}

/// Keep the returned handle alive: dropping it stops SNTP.
pub fn start_sntp() -> Result<EspSntp<'static>> {
    Ok(EspSntp::new_default()?)
}

/// Publishes `<hostname>.local` and an HTTP service on port 80. Keep the handle alive.
pub fn start_mdns(hostname: &str) -> Result<EspMdns> {
    let mut mdns = EspMdns::take()?;
    mdns.set_hostname(hostname)?;
    mdns.set_instance_name("esp-shazam")?;
    mdns.add_service(Some("esp-shazam"), "_http", "_tcp", 80, &[("path", "/")])?;
    Ok(mdns)
}
