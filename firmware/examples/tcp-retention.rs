// ABOUTME: Samples actual TCP PCB retention through the lwIP task while production runs.
// ABOUTME: Reports TCP send state, counted link-output failures and public signal/channel readings.

// Private PCB lists/layouts depend on the pinned ESP-IDF 5.5.5 lwIP headers. Generated
// bindings avoid copied ABI assumptions. Flash this example, run real stream-churn,
// and compare valid peaks/counts with production close/slot logs and heap telemetry.
// Require coherent valid samples through admission, stall, close and recovery; invalid
// or truncated observations cannot prove reclamation. TIME_WAIT/control PCBs differ
// from queued audio payload. Host-retained sockets do not count board PCBs.
// Restore production with scripts/flash afterward. No callback logs or allocations.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys::{self, tcp_retention as lwip};

use esp_shazam_firmware::threads;

const LIST_LIMIT: usize = 256;
const STATE_COUNT: usize = 11;
const COUNT_LEN: usize = 4 + STATE_COUNT;
const SEND_ROWS: usize = 4;
const SEND_FIELDS: usize = 18;
const SEGMENT_LIMIT: usize = 64;
// Rows contain remote port, state, MSS, windows, queue space, sequence progress,
// retransmission state and bounded segment totals. No payload or IP address is read.
static SEND: [[AtomicU32; SEND_FIELDS]; SEND_ROWS] =
    [const { [const { AtomicU32::new(0) }; SEND_FIELDS] }; SEND_ROWS];
static SEND_COUNT: AtomicU32 = AtomicU32::new(0);
static SEND_INVALID: AtomicBool = AtomicBool::new(false);

/// Called only with the TCP/IP task owning the PCB/segment lists. Segment lengths
/// count TCP data bytes, while snd_queuelen separately counts pbufs, not segments.
unsafe fn queue_totals(mut segment: *mut lwip::tcp_seg) -> (u32, u32, bool) {
    let mut count = 0;
    let mut bytes = 0;
    for _ in 0..SEGMENT_LIMIT {
        if segment.is_null() {
            break;
        }
        // SAFETY: callback owns the stable generated lwIP list; no pointer escapes.
        unsafe {
            count += 1;
            bytes += u32::from((*segment).len);
            segment = (*segment).next;
        }
    }
    (count, bytes, !segment.is_null())
}
// 0 idle, 1 callback owns the storage, 2 complete. Static storage outlives late callbacks.
static PHASE: AtomicU32 = AtomicU32::new(0);
static INVALID: AtomicBool = AtomicBool::new(false);
static COUNTS: [AtomicU32; COUNT_LEN] = [const { AtomicU32::new(0) }; COUNT_LEN];
static HTTP_COUNTS: [AtomicU32; STATE_COUNT] = [const { AtomicU32::new(0) }; STATE_COUNT];

// Fixture-exclusive hook invariant: production and linked mDNS/SNTP use socket/API
// messages on the sole TCP/IP task (core locking is disabled). No bridge/PPP/raw
// output or other linkoutput hook is installed. Installation and original access
// occur only on that task; production run retains the STA netif until reboot.
struct OriginalOutput(UnsafeCell<lwip::netif_linkoutput_fn>);
// SAFETY: only the sole TCP/IP task accesses this cell, including installation.
unsafe impl Sync for OriginalOutput {}
static ORIGINAL_OUTPUT: OriginalOutput = OriginalOutput(UnsafeCell::new(None));
// 0 not attempted, 1 installed, 2 rejected. Callback publishes installation outcome.
static OUTPUT_STATE: AtomicU32 = AtomicU32::new(0);
// Single/chained calls, then single/chained ERR_MEM, then single/chained other errors.
static OUTPUT_COUNTS: [AtomicU32; 6] = [const { AtomicU32::new(0) }; 6];

unsafe fn install_output_counter() {
    if OUTPUT_STATE.load(Ordering::Relaxed) != 0 {
        return;
    }
    // SAFETY: pinned esp_netif_lwip.c212-229 executes this getter directly when
    // already on TCP/IP; get_netif_impl495-500 only returns the retained pointer.
    // Its ESP_LOGD is disabled by pinned LOG_MAXIMUM_LEVEL=3 (no local override).
    unsafe {
        let handle = sys::esp_netif_get_handle_from_ifkey(c"WIFI_STA_DEF".as_ptr());
        let netif = sys::esp_netif_get_netif_impl(handle).cast::<lwip::netif>();
        if netif.is_null() || (*ORIGINAL_OUTPUT.0.get()).is_some() {
            OUTPUT_STATE.store(2, Ordering::Release);
            return;
        }
        let Some(original) = (*netif).linkoutput else {
            OUTPUT_STATE.store(2, Ordering::Release);
            return;
        };
        if original as usize == count_output as *const () as usize {
            OUTPUT_STATE.store(2, Ordering::Release);
            return;
        }
        // Save the typed original before publishing the wrapper on the same task.
        *ORIGINAL_OUTPUT.0.get() = Some(original);
        (*netif).linkoutput = Some(count_output);
        OUTPUT_STATE.store(1, Ordering::Release);
    }
}

unsafe extern "C" fn count_output(netif: *mut lwip::netif, p: *mut lwip::pbuf) -> lwip::err_t {
    // SAFETY: installed only on the sole TCP/IP task; lwIP supplies a live pbuf
    // and netif. Read only the next pointer before calling the original once.
    unsafe {
        let Some(original) = *ORIGINAL_OUTPUT.0.get() else {
            OUTPUT_STATE.store(2, Ordering::Release);
            return lwip::err_enum_t_ERR_IF as lwip::err_t;
        };
        let chain = usize::from(!(*p).next.is_null());
        OUTPUT_COUNTS[chain].fetch_add(1, Ordering::Relaxed);
        let result = original(netif, p);
        if result == lwip::err_enum_t_ERR_MEM as lwip::err_t {
            OUTPUT_COUNTS[2 + chain].fetch_add(1, Ordering::Relaxed);
        } else if result != lwip::err_enum_t_ERR_OK as lwip::err_t {
            OUTPUT_COUNTS[4 + chain].fetch_add(1, Ordering::Relaxed);
        }
        result
    }
}

unsafe extern "C" fn sample(_: *mut std::ffi::c_void) {
    // SAFETY: sample runs in the TCP/IP task, before any list observation.
    unsafe { install_output_counter() };
    let mut counts = [0u32; COUNT_LEN];
    let mut http_counts = [0u32; STATE_COUNT];
    let mut invalid = false;
    let mut send = [[0u32; SEND_FIELDS]; SEND_ROWS];
    let mut send_count = 0usize;
    let mut send_invalid = false;
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
                // TIME_WAIT and listen records are excluded from sender diagnostics.
                if port == 8000 && index < 2 {
                    if send_count < SEND_ROWS {
                        let (unsent_count, unsent_bytes, unsent_truncated) =
                            queue_totals((*pcb).unsent);
                        let (unacked_count, unacked_bytes, unacked_truncated) =
                            queue_totals((*pcb).unacked);
                        send_invalid |= unsent_truncated || unacked_truncated;
                        send[send_count] = [
                            u32::from((*pcb).remote_port),
                            (*pcb).state as u32,
                            u32::from((*pcb).mss),
                            (*pcb).snd_wnd as u32,
                            (*pcb).cwnd as u32,
                            (*pcb).snd_buf as u32,
                            u32::from((*pcb).snd_queuelen),
                            (*pcb).lastack,
                            (*pcb).snd_nxt,
                            (*pcb).snd_lbb,
                            u32::from((*pcb).nrtx),
                            i32::from((*pcb).rtime) as u32,
                            i32::from((*pcb).rto) as u32,
                            u32::from((*pcb).persist_backoff),
                            unsent_count,
                            unsent_bytes,
                            unacked_count,
                            unacked_bytes,
                        ];
                    } else {
                        send_invalid = true;
                    }
                    send_count += 1;
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
    for (targets, row) in SEND.iter().zip(send) {
        for (target, value) in targets.iter().zip(row) {
            target.store(value, Ordering::Relaxed);
        }
    }
    SEND_COUNT.store(send_count as u32, Ordering::Relaxed);
    SEND_INVALID.store(send_invalid || invalid, Ordering::Relaxed);
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
    let mut latest_send = None;
    let mut previous_output = [0u32; 6];
    loop {
        match PHASE.load(Ordering::Acquire) {
            2 => {
                let send_valid = !overdue
                    && requested.elapsed() <= Duration::from_millis(500)
                    && !SEND_INVALID.load(Ordering::Relaxed);
                let rows: [[u32; SEND_FIELDS]; SEND_ROWS] = std::array::from_fn(|row| {
                    std::array::from_fn(|field| SEND[row][field].load(Ordering::Relaxed))
                });
                latest_send = Some((send_valid, SEND_COUNT.load(Ordering::Relaxed), rows));
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
            if let Some((send_valid, count, rows)) = latest_send {
                log::info!("tcp-retention: send_valid={send_valid} send_rows={count} row_limit={SEND_ROWS} segment_limit={SEGMENT_LIMIT}");
                if send_valid {
                    for row in rows.iter().take(count as usize) {
                        log::info!("tcp-retention: send remote_port={} state={} mss={} snd_wnd={} cwnd={} snd_buf={} snd_queuelen={} lastack={} snd_nxt={} snd_lbb={} nrtx={} rtime_ticks={} rto_ticks={} persist_backoff={} unsent_segments={} unsent_bytes={} unacked_segments={} unacked_bytes={}",
                            row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7], row[8], row[9], row[10], row[11] as i32, row[12] as i32, row[13], row[14], row[15], row[16], row[17]);
                    }
                }
            }
            // These independent wrapping counters are sampled, not a coherent packet record.
            // wlanif.c91-115: single ERR_MEM maps driver ESP_ERR_NO_MEM; a chain
            // also permits coalescing pbuf allocation failure. Neither identifies a pool.
            let output: [u32; 6] =
                std::array::from_fn(|i| OUTPUT_COUNTS[i].load(Ordering::Relaxed));
            let delta: [u32; 6] =
                std::array::from_fn(|i| output[i].wrapping_sub(previous_output[i]));
            log::info!("tcp-retention: output_hook={} output_delta={delta:?} output_total={output:?} (all STA ports; single,chain,single_ERR_MEM,chain_ERR_MEM,single_other,chain_other; wrapping counters)", OUTPUT_STATE.load(Ordering::Acquire));
            previous_output = output;
            latest_send = None;
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
