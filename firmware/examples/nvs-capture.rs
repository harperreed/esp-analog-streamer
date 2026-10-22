// ABOUTME: Writes bounded real NVS blobs while the production capture and streams run unchanged.
// ABOUTME: Uses only a diagnostic namespace/key and emits baseline, write and recovery markers.

// Flash this example only with recognition disabled. Capture UART/API counters and a full
// decoded stream-soak run with one listener, then repeat with two. Start listeners during
// BASELINE and include OPEN, all WRITE markers, CLEANUP and RECOVERED in the evidence.
// Success needs verified readbacks, actual decoded sample counts, unchanged capture fault
// counters and generation, and no reset. This fixture's logs alone cannot prove continuity.

use std::time::Duration;

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys::{self, esp};
use esp_shazam_firmware::{config, threads};

const _: () = assert!(
    !config::CONFIG.shazam_enabled,
    "nvs-capture requires shazam_enabled=false"
);

const WRITES: u32 = 120;
const BYTES: usize = 1024;

struct DiagnosticNvs(sys::nvs_handle_t);

impl Drop for DiagnosticNvs {
    fn drop(&mut self) {
        // SAFETY: this handle was opened successfully and is closed exactly once.
        unsafe { sys::nvs_close(self.0) };
    }
}

fn writes(nvs: &DiagnosticNvs) -> anyhow::Result<()> {
    let mut payload = [0u8; BYTES];
    let mut readback = [0u8; BYTES];
    for sequence in 0..WRITES {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_add(sequence as u8);
        }
        payload[..4].copy_from_slice(&sequence.to_le_bytes());
        marker("WRITE_BEGIN", sequence);
        let started = micros();
        let mut length = readback.len();
        // SAFETY: live diagnostic-only handle/key, and valid fixed-size source/destination
        // buffers whose lengths are passed to NVS. Only this thread uses the handle.
        unsafe {
            esp!(sys::nvs_set_blob(
                nvs.0,
                c"payload".as_ptr(),
                payload.as_ptr().cast(),
                payload.len(),
            ))?;
            esp!(sys::nvs_commit(nvs.0))?;
            esp!(sys::nvs_get_blob(
                nvs.0,
                c"payload".as_ptr(),
                readback.as_mut_ptr().cast(),
                &mut length,
            ))?;
        }
        anyhow::ensure!(
            length == BYTES && readback == payload,
            "NVS readback mismatch"
        );
        log::info!(
            "nvs-capture: marker=WRITE_END sequence={sequence} ms={} elapsed_us={} bytes={BYTES} readback=true",
            micros() / 1000,
            micros().saturating_sub(started),
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

fn experiment() -> anyhow::Result<()> {
    // run() owns NVS initialization. Open its initialized partition directly instead of
    // taking a second singleton or invoking an initialization/partition-erasure fallback.
    marker("OPEN_BEGIN", 0);
    let mut handle = 0;
    // SAFETY: namespace is diagnostic-only, nul-terminated and below NVS's name limit.
    unsafe {
        esp!(sys::nvs_open(
            c"capture-iram".as_ptr(),
            sys::nvs_open_mode_t_NVS_READWRITE,
            &mut handle,
        ))?;
    }
    let nvs = DiagnosticNvs(handle);
    marker("OPEN_END", 0);
    let result = writes(&nvs);
    // Cleanup is also real flash work and belongs inside the capture measurement window.
    marker("CLEANUP_BEGIN", WRITES);
    // SAFETY: erase only the owned diagnostic key, never settings or the partition itself.
    let cleanup = unsafe {
        let erased = sys::nvs_erase_key(nvs.0, c"payload".as_ptr());
        if erased == sys::ESP_ERR_NVS_NOT_FOUND {
            Ok(())
        } else {
            esp!(erased).and_then(|()| esp!(sys::nvs_commit(nvs.0)))
        }
    };
    if let Err(error) = &cleanup {
        log::error!(
            "nvs-capture: marker=CLEANUP_FAILED ms={} error={error}",
            micros() / 1000
        );
    } else {
        marker("CLEANUP_END", WRITES);
    }
    result?;
    cleanup?;
    std::thread::sleep(Duration::from_secs(20));
    marker("RECOVERED", WRITES);
    Ok(())
}

fn marker(name: &str, sequence: u32) {
    log::info!(
        "nvs-capture: marker={name} sequence={sequence} ms={} writes={WRITES} bytes={BYTES}",
        micros() / 1000,
    );
}

fn micros() -> u64 {
    // SAFETY: ESP-IDF starts this monotonic timer before entering main.
    unsafe { sys::esp_timer_get_time() as u64 }
}

fn main() -> anyhow::Result<()> {
    threads::ThreadSpec {
        name: c"nvs-capture",
        core: Core::Core1,
        priority: 4,
        stack_bytes: 8 * 1024,
    }
    .spawn(|| {
        // Give production startup time to initialize logging, NVS, capture and Wi-Fi.
        std::thread::sleep(Duration::from_secs(15));
        marker("BASELINE", 0);
        // Hosts get 25 seconds to admit one/two streams and establish stable counters.
        std::thread::sleep(Duration::from_secs(25));
        if let Err(error) = experiment() {
            log::error!(
                "nvs-capture: marker=FAILED ms={} error={error}",
                micros() / 1000
            );
        }
    })?;
    esp_shazam_firmware::run()
}
