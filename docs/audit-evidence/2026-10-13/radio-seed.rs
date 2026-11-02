// ABOUTME: Seeds only the persistent station b/g protocol for a throwaway reset-restoration test.
// ABOUTME: Reads before writing, reports numeric receipts, and idles without configuring credentials.

use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{self, esp, EspError};
use esp_idf_svc::wifi::EspWifi;

const DIAGNOSTIC_PROTOCOL: u8 = (sys::WIFI_PROTOCOL_11B | sys::WIFI_PROTOCOL_11G) as u8;

fn main() {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    receipt(0, 0);
    // Keep the initialized driver owned while idle. No start/connect/configuration call.
    let _driver = match seed() {
        Ok(driver) => Some(driver),
        Err(error) => {
            receipt(99, error.code());
            None
        }
    };
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn seed() -> Result<EspWifi<'static>, EspError> {
    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    // Preserve existing credentials: NVS initialization errors must not trigger an erase.
    let nvs = EspDefaultNvsPartition::take_with(false)?;
    let driver = EspWifi::new(peripherals.modem, sysloop, Some(nvs))?;
    let before = read_protocol()?;
    receipt(1, i32::from(before));
    // SAFETY: EspWifi initialized the driver; FLASH is the pinned IDF storage enum.
    esp!(unsafe { sys::esp_wifi_set_storage(sys::wifi_storage_t_WIFI_STORAGE_FLASH) })?;
    receipt(2, 0);
    // SAFETY: initialized station interface; only documented b/g bits are supplied.
    esp!(unsafe {
        sys::esp_wifi_set_protocol(sys::wifi_interface_t_WIFI_IF_STA, DIAGNOSTIC_PROTOCOL)
    })?;
    receipt(3, 0);
    let after = read_protocol()?;
    receipt(4, i32::from(after));
    if after != DIAGNOSTIC_PROTOCOL {
        return Err(EspError::from_infallible::<{ sys::ESP_ERR_INVALID_STATE }>());
    }
    receipt(5, 0);
    Ok(driver)
}

fn read_protocol() -> Result<u8, EspError> {
    let mut protocol = 0;
    // SAFETY: driver is initialized and the output points to a writable byte.
    esp!(unsafe {
        sys::esp_wifi_get_protocol(sys::wifi_interface_t_WIFI_IF_STA, &mut protocol)
    })?;
    Ok(protocol)
}

fn receipt(phase: u8, value: i32) {
    // SAFETY: these public system queries take no pointers and require no Wi-Fi connection.
    let (reset, ms, heap) = unsafe {
        (
            sys::esp_reset_reason(),
            sys::esp_timer_get_time() / 1_000,
            sys::esp_get_free_heap_size(),
        )
    };
    log::info!("radio-seed: phase={phase} value={value} reset={reset} ms={ms} heap={heap}");
}
