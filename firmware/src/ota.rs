// ABOUTME: Over-the-air updates: streams an uploaded app image into the other slot behind a shared token, restarts
// ABOUTME: into it, and marks a freshly booted image valid once it is healthy so the bootloader can roll back otherwise.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use esp_idf_svc::ota::{EspOta, SlotState};
use esp_idf_svc::sys;
use esp_shazam_core::network_lifecycle::Network;
use esp_shazam_core::ota_image::{self, AppDescription, ImageError, HEAD_BYTES};

/// One upload at a time; a second is refused, not queued.
static UPLOADING: AtomicBool = AtomicBool::new(false);
/// Set once this boot's image was marked valid (or did not need marking).
static VALIDATED: AtomicBool = AtomicBool::new(false);
/// A freshly booted image proves itself with this much Wi-Fi uptime; a panic, watchdog or reset before that
/// returns the bootloader to the previous slot (CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE).
const HEALTHY_AFTER: Duration = Duration::from_secs(60);
const CHUNK_BYTES: usize = 4096;

pub enum Refusal {
    Token,
    Image(ImageError),
    Busy,
    Disabled,
    Length,
}

#[derive(serde::Serialize)]
pub struct Status {
    pub running: String,
    pub boot: String,
    pub running_state: &'static str,
    pub validated: bool,
    pub enabled: bool,
    pub update_slot: String,
    pub update_slot_bytes: usize,
}

fn state_name(state: &SlotState) -> &'static str {
    match state {
        SlotState::Factory => "factory",
        SlotState::Unverified => "pending_verify",
        SlotState::Valid => "valid",
        SlotState::Invalid => "invalid",
        SlotState::Unknown => "unknown",
    }
}

fn slot_bytes(label: &str) -> usize {
    // esp-idf-svc's Slot carries no size; the partition table does.
    let label = std::ffi::CString::new(label).unwrap_or_default();
    let partition = unsafe {
        sys::esp_partition_find_first(
            sys::esp_partition_type_t_ESP_PARTITION_TYPE_APP,
            sys::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY,
            label.as_ptr(),
        )
    };
    if partition.is_null() {
        0
    } else {
        unsafe { (*partition).size as usize }
    }
}

pub fn status(token_configured: bool) -> Result<Status> {
    let ota = EspOta::new().context("OTA handle")?;
    let running = ota.get_running_slot()?;
    let boot = ota.get_boot_slot()?;
    let update = ota.get_update_slot()?;
    Ok(Status {
        running: running.label.to_string(),
        boot: boot.label.to_string(),
        running_state: state_name(&running.state),
        validated: VALIDATED.load(Ordering::Relaxed),
        enabled: token_configured && update.label != running.label,
        update_slot_bytes: slot_bytes(&update.label),
        update_slot: update.label.to_string(),
    })
}

/// Marks this boot's image valid after HEALTHY_AFTER of continuous Wi-Fi; runs on its own small thread.
pub fn mark_valid_when_healthy(network: Network) {
    let _ = std::thread::Builder::new()
        .name("ota-health".into())
        .stack_size(4096)
        .spawn(move || {
            let mut up_since: Option<Instant> = None;
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let connected = network.snapshot().address.is_some();
                up_since = match (connected, up_since) {
                    (false, _) => None,
                    (true, None) => Some(Instant::now()),
                    (true, since) => since,
                };
                if up_since.is_some_and(|since| since.elapsed() >= HEALTHY_AFTER) {
                    match EspOta::new().and_then(|mut ota| ota.mark_running_slot_valid()) {
                        Ok(()) => log::info!("ota: running image marked valid"),
                        Err(error) => {
                            log::warn!("ota: cannot mark the running image valid: {error}")
                        }
                    }
                    VALIDATED.store(true, Ordering::Relaxed);
                    return;
                }
            }
        });
}

/// Streams `declared` bytes from `read` into the update slot. The token and the image head are judged before a
/// byte is written; a failure after that abandons the slot and leaves the running image untouched.
pub fn upload(
    configured_token: &str,
    offered_token: Option<&str>,
    declared: Option<usize>,
    mut read: impl FnMut(&mut [u8]) -> std::io::Result<usize>,
) -> Result<Result<AppDescription, Refusal>> {
    if configured_token.is_empty() {
        return Ok(Err(Refusal::Disabled));
    }
    if !ota_image::token_matches(configured_token, offered_token.unwrap_or("")) {
        return Ok(Err(Refusal::Token));
    }
    let Some(declared) = declared else {
        return Ok(Err(Refusal::Length));
    };
    if UPLOADING.swap(true, Ordering::AcqRel) {
        return Ok(Err(Refusal::Busy));
    }
    let outcome = stream(declared, &mut read);
    UPLOADING.store(false, Ordering::Release);
    outcome
}

fn fill(
    read: &mut impl FnMut(&mut [u8]) -> std::io::Result<usize>,
    buf: &mut [u8],
) -> Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        let n = read(&mut buf[got..]).context("upload body read")?;
        if n == 0 {
            break;
        }
        got += n;
    }
    Ok(got)
}

fn stream(
    declared: usize,
    read: &mut impl FnMut(&mut [u8]) -> std::io::Result<usize>,
) -> Result<Result<AppDescription, Refusal>> {
    let mut ota = EspOta::new().context("OTA handle")?;
    let slot = ota.get_update_slot()?;
    let slot_size = slot_bytes(&slot.label);
    let mut head = vec![0u8; HEAD_BYTES.min(declared)];
    let got = fill(read, &mut head)?;
    head.truncate(got);
    let description = match ota_image::inspect(&head, declared, slot_size) {
        Ok(description) => description,
        Err(error) => return Ok(Err(Refusal::Image(error))),
    };
    log::info!(
        "ota: writing {declared} bytes (version {}, elf {}) to {}",
        description.version,
        ota_image::hex(&description.elf_sha256[..4]),
        slot.label
    );
    let mut update = ota.initiate_update().context("begin update")?;
    let written = (|| -> Result<()> {
        update.write(&head).context("write head")?;
        let mut remaining = declared - head.len();
        let mut chunk = vec![0u8; CHUNK_BYTES];
        while remaining > 0 {
            let want = remaining.min(CHUNK_BYTES);
            let got = fill(read, &mut chunk[..want])?;
            if got == 0 {
                return Err(anyhow!("upload ended {remaining} bytes short"));
            }
            update.write(&chunk[..got]).context("write chunk")?;
            remaining -= got;
        }
        Ok(())
    })();
    if let Err(error) = written {
        let _ = update.abort();
        return Err(error);
    }
    update.complete().context("finish update")?;
    log::info!("ota: image complete; restarting into {}", slot.label);
    let _ = std::thread::Builder::new()
        .name("ota-restart".into())
        .stack_size(2048)
        .spawn(|| {
            std::thread::sleep(Duration::from_secs(1));
            unsafe { sys::esp_restart() }
        });
    Ok(Ok(description))
}
