// ABOUTME: Samples actual TCP PCB retention through the lwIP task while production runs.
// ABOUTME: Reports port-8000/80 states and public signal/channel readings without hooks or routes.

// Private PCB lists/layouts depend on the pinned ESP-IDF 5.5.5 lwIP headers. Generated
// bindings avoid copied ABI assumptions. Flash this example, run real stream-churn,
// and compare valid peaks/counts with production close/slot logs and heap telemetry.
// Require coherent valid samples through admission, stall, close and recovery; invalid
// or truncated observations cannot prove reclamation. TIME_WAIT/control PCBs differ
// from queued audio payload. Host-retained sockets do not count board PCBs.
// Restore production with scripts/flash afterward. No callback logs or allocations.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys::{self, tcp_retention as lwip};

#[path = "../src/threads.rs"]
mod threads;

const LIST_LIMIT: usize = 256;
const STATE_COUNT: usize = 11;
const COUNT_LEN: usize = 4 + STATE_COUNT;
// 0 idle, 1 callback owns the storage, 2 complete. Static storage outlives late callbacks.
static PHASE: AtomicU32 = AtomicU32::new(0);
static INVALID: AtomicBool = AtomicBool::new(false);
static COUNTS: [AtomicU32; COUNT_LEN] = [const { AtomicU32::new(0) }; COUNT_LEN];
static HTTP_COUNTS: [AtomicU32; STATE_COUNT] = [const { AtomicU32::new(0) }; STATE_COUNT];

unsafe extern "C" fn sample(_: *mut std::ffi::c_void) {
    let mut counts = [0u32; COUNT_LEN];
    let mut http_counts = [0u32; STATE_COUNT];
    let mut invalid = false;
    // SAFETY: lwIP invokes this callback in its sole TCP/IP task. No list mutation or
    // pointer escape occurs; the generated types match this exact SDK configuration.
    unsafe {
        for (index, mut pcb) in [
            lwip::tcp_bound_pcbs,
            lwip::tcp_active_pcbs,
            lwip::tcp_tw_pcbs,
        ]
        .into_iter()
        .enumerate()
        {
            for _ in 0..LIST_LIMIT {
                if pcb.is_null() {
                    break;
                }
                counts[index] += 1;
                let port = (*pcb).local_port;
                if port == 8000 || port == 80 {
                    let state = (*pcb).state as usize;
                    if state < STATE_COUNT {
                        if port == 8000 {
                            counts[4 + state] += 1;
                        } else {
                            http_counts[state] += 1;
                        }
                    } else {
                        invalid = true;
                    }
                }
                pcb = (*pcb).next;
            }
            invalid |= !pcb.is_null();
        }
        let mut pcb = lwip::tcp_listen_pcbs.listen_pcbs;
        for _ in 0..LIST_LIMIT {
            if pcb.is_null() {
                break;
            }
            counts[3] += 1;
            let port = (*pcb).local_port;
            if port == 8000 || port == 80 {
                let state = (*pcb).state as usize;
                if state < STATE_COUNT {
                    if port == 8000 {
                        counts[4 + state] += 1;
                    } else {
                        http_counts[state] += 1;
                    }
                } else {
                    invalid = true;
                }
            }
            pcb = (*pcb).next;
        }
        invalid |= !pcb.is_null();
    }
    for (target, value) in COUNTS.iter().zip(counts) {
        target.store(value, Ordering::Relaxed);
    }
    for (target, value) in HTTP_COUNTS.iter().zip(http_counts) {
        target.store(value, Ordering::Relaxed);
    }
    INVALID.store(invalid, Ordering::Relaxed);
    PHASE.store(2, Ordering::Release);
}

fn observe() {
    std::thread::sleep(Duration::from_secs(15));
    let mut requested = Instant::now();
    let mut reporting = Instant::now();
    let mut link_reporting = Instant::now();
    let mut overdue = false;
    let mut valid = 0u32;
    let mut failed = 0u32;
    let mut peaks = [0u32; COUNT_LEN];
    let mut latest = None;
    let mut http_peaks = [0u32; STATE_COUNT];
    let mut latest_http = None;
    loop {
        match PHASE.load(Ordering::Acquire) {
            2 => {
                if !overdue
                    && requested.elapsed() <= Duration::from_millis(500)
                    && !INVALID.load(Ordering::Relaxed)
                {
                    let counts: [u32; COUNT_LEN] =
                        std::array::from_fn(|index| COUNTS[index].load(Ordering::Relaxed));
                    let http_counts: [u32; STATE_COUNT] =
                        std::array::from_fn(|index| HTTP_COUNTS[index].load(Ordering::Relaxed));
                    for (peak, count) in peaks.iter_mut().zip(counts) {
                        *peak = (*peak).max(count);
                    }
                    latest = Some(counts);
                    for (peak, count) in http_peaks.iter_mut().zip(http_counts) {
                        *peak = (*peak).max(count);
                    }
                    latest_http = Some(http_counts);
                    valid += 1;
                } else {
                    if !overdue {
                        failed += 1;
                    }
                    log::warn!("tcp-retention: invalid completed sample overdue={} truncated_or_unknown_state={}",
                        overdue || requested.elapsed() > Duration::from_millis(500), INVALID.load(Ordering::Relaxed));
                }
                PHASE.store(0, Ordering::Release);
            }
            1 if !overdue && requested.elapsed() > Duration::from_millis(500) => {
                overdue = true;
                failed += 1;
                log::warn!("tcp-retention: invalid overdue sample; pending storage retained");
            }
            _ => {}
        }
        if PHASE.load(Ordering::Acquire) == 0 {
            if link_reporting.elapsed() >= Duration::from_secs(10) {
                // Outside the lwIP callback, before starting its deadline. This public query
                // reports signal/channel only; it does not measure delivered throughput.
                let mut info = sys::wifi_ap_record_t::default();
                // SAFETY: generated pinned-IDF record is writable and lives through the call.
                let result = unsafe { sys::esp_wifi_sta_get_ap_info(&mut info) };
                if result == sys::ESP_OK {
                    log::info!(
                        "tcp-retention: wifi_link result={result} rssi_dbm={} primary_channel={}",
                        info.rssi,
                        info.primary
                    );
                } else {
                    log::info!(
                        "tcp-retention: wifi_link result={result} (signal/channel unavailable)"
                    );
                }
                link_reporting = Instant::now();
            }
            requested = Instant::now();
            overdue = false;
            PHASE.store(1, Ordering::Release);
            // SAFETY: callback and static atomics remain alive for the whole firmware lifetime.
            // ERR_MEM means no callback was queued; only then may the observer reclaim storage.
            let result = unsafe { lwip::tcpip_try_callback(Some(sample), std::ptr::null_mut()) };
            if result != lwip::err_enum_t_ERR_OK as lwip::err_t {
                PHASE.store(0, Ordering::Release);
                failed += 1;
                log::warn!("tcp-retention: invalid submission err={result}");
            }
        }
        if reporting.elapsed() >= Duration::from_secs(1) {
            log::info!("tcp-retention: valid_samples={valid} invalid_samples={failed} pending={} latest={latest:?} peaks={peaks:?} port80_states={latest_http:?} port80_max={http_peaks:?} (lists bound,active,time_wait,listen; both ports states CLOSED,LISTEN,SYN_SENT,SYN_RCVD,ESTABLISHED,FIN_WAIT_1,FIN_WAIT_2,CLOSE_WAIT,CLOSING,LAST_ACK,TIME_WAIT)", PHASE.load(Ordering::Acquire));
            reporting = Instant::now();
            valid = 0;
            failed = 0;
            peaks = [0; COUNT_LEN];
            latest = None;
            http_peaks = [0; STATE_COUNT];
            latest_http = None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn main() -> anyhow::Result<()> {
    threads::ThreadSpec {
        name: c"tcp-retention",
        core: Core::Core1,
        priority: 3,
        stack_bytes: 4 * 1024,
    }
    .spawn(observe)?;
    esp_shazam_firmware::run()
}
