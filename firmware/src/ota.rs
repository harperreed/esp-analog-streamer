// ABOUTME: Over-the-air updates: streams an uploaded app image into the other slot behind a shared token, restarts
// ABOUTME: into it, and marks a freshly booted image valid once it is healthy so the bootloader can roll back otherwise.
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::ota::{EspOta, SlotState};
use esp_idf_svc::sys;
use esp_shazam_core::network_lifecycle::Network;
use esp_shazam_core::ota_image::{self, AppDescription, ImageError, HEAD_BYTES};

use crate::threads::ThreadSpec;

/// esp-idf-svc allows one `EspOta` per process (`EspOta::new` refuses a second while one lives), so this is it,
/// opened on first use. Holding the guard is the busy signal: an upload holds it for its whole write, and a
/// second upload is refused, not queued. Everyone else takes it briefly.
static OTA: Mutex<Option<EspOta>> = Mutex::new(None);
/// A freshly booted image proves itself with this much Wi-Fi uptime; a panic, watchdog or reset before that
/// returns the bootloader to the previous slot (CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE).
const HEALTHY_AFTER: Duration = Duration::from_secs(60);
const CHUNK_BYTES: usize = 4096;
/// Both threads sleep for nearly their whole lives, so they run on core 0 at priority 2, the Sonos worker's
/// level: below the capture thread (24) and ESP-IDF's network tasks there, and off core 1, which the encoder,
/// listeners and recognizer share. 6 KiB covers the health thread's lock and mark; 2 KiB a sleep and a restart.
const HEALTH_THREAD: ThreadSpec = ThreadSpec {
    name: c"ota-health",
    core: Core::Core0,
    priority: 2,
    stack_bytes: 6144,
};
const RESTART_THREAD: ThreadSpec = ThreadSpec {
    name: c"ota-restart",
    stack_bytes: 2048,
    ..HEALTH_THREAD
};

pub enum Refusal {
    Token,
    Image(ImageError),
    Busy,
    Disabled,
    Length,
    /// The running image has not marked itself valid yet; `esp_ota_begin` refuses the update slot until it does.
    Pending,
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

type Guard = MutexGuard<'static, Option<EspOta>>;

/// Takes the handle without waiting; `None` means an upload holds it.
fn try_claim() -> Result<Option<Guard>> {
    match OTA.try_lock() {
        Ok(guard) => Ok(Some(guard)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Poisoned(_)) => Err(anyhow!("OTA handle poisoned")),
    }
}

/// Waits for the handle; an upload in progress finishes first.
fn claim() -> Result<Guard> {
    OTA.lock().map_err(|_| anyhow!("OTA handle poisoned"))
}

/// The process's one `EspOta`, opened on the first call.
fn open(slot: &mut Option<EspOta>) -> Result<&mut EspOta> {
    if slot.is_none() {
        *slot = Some(EspOta::new().context("OTA handle")?);
    }
    Ok(slot.as_mut().expect("opened above"))
}

/// `None` while an upload holds the handle; an httpd handler must never wait on it.
pub fn status(token_configured: bool) -> Result<Option<Status>> {
    let Some(mut guard) = try_claim()? else {
        return Ok(None);
    };
    let ota = open(&mut guard)?;
    let running = ota.get_running_slot()?;
    let boot = ota.get_boot_slot()?;
    let update = ota.get_update_slot()?;
    Ok(Some(Status {
        running: running.label.to_string(),
        boot: boot.label.to_string(),
        running_state: state_name(&running.state),
        // otadata is the one authority: the field stays so the page and the pusher keep their contract.
        validated: matches!(running.state, SlotState::Valid),
        enabled: token_configured && update.label != running.label,
        update_slot_bytes: slot_bytes(&update.label),
        update_slot: update.label.to_string(),
    }))
}

/// Marks this boot's image valid after HEALTHY_AFTER of continuous Wi-Fi. The thread (and its internal-RAM stack)
/// exists only while the running slot is still pending verification: an image flashed over USB or already marked
/// valid has nothing to prove, and internal RAM is the scarcest thing on this board.
pub fn mark_valid_when_healthy(network: Network) {
    // The first lock of an ESP-IDF std mutex allocates; this one happens here, on the main thread at boot.
    let running = claim().and_then(|mut guard| Ok(open(&mut guard)?.get_running_slot()?));
    match running {
        Ok(slot) if matches!(slot.state, SlotState::Unverified) => {}
        Ok(slot) => {
            log::info!(
                "ota: running slot {} is {}; nothing to verify",
                slot.label,
                state_name(&slot.state)
            );
            return;
        }
        Err(error) => {
            log::warn!("ota: cannot read the running slot ({error}); not watching for validity");
            return;
        }
    }
    let spawned = HEALTH_THREAD.spawn(move || {
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
                // A failed mark is not success: keep ticking so the next tick retries, and keep
                // warning, since a mark that never lands means a rollback. Waiting on the handle is
                // fine here: an upload in progress finishes first, then this slot is marked.
                let marked =
                    claim().and_then(|mut guard| Ok(open(&mut guard)?.mark_running_slot_valid()?));
                match marked {
                    Ok(()) => {
                        log::info!("ota: running image marked valid");
                        return;
                    }
                    Err(error) => {
                        log::warn!("ota: cannot mark the running image valid: {error}")
                    }
                }
            }
        }
    });
    if let Err(error) = spawned {
        // Without the thread the image never marks itself valid and the next reset rolls back.
        log::error!("ota: cannot spawn the health thread: {error}");
    }
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
    let Some(mut guard) = try_claim()? else {
        return Ok(Err(Refusal::Busy));
    };
    let ota = open(&mut guard)?;
    // ESP-IDF refuses esp_ota_begin while this boot is still proving itself (ESP_ERR_OTA_ROLLBACK_INVALID_STATE).
    // Judged here, from otadata, before the body is read: the pusher then gets a status instead of a closed socket.
    if matches!(ota.get_running_slot()?.state, SlotState::Unverified) {
        return Ok(Err(Refusal::Pending));
    }
    stream(ota, declared, &mut read)
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
    ota: &mut EspOta,
    declared: usize,
    read: &mut impl FnMut(&mut [u8]) -> std::io::Result<usize>,
) -> Result<Result<AppDescription, Refusal>> {
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
    // The boot slot is already set, so after a failed spawn the next reset still boots the new image.
    if let Err(error) = RESTART_THREAD.spawn(|| {
        std::thread::sleep(Duration::from_secs(1));
        unsafe { sys::esp_restart() }
    }) {
        log::error!("ota: cannot spawn the restart thread: {error}");
    }
    Ok(Ok(description))
}
