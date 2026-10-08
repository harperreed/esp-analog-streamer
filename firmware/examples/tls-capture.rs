// ABOUTME: Runs three real receive-only TLS/WebSocket cycles beside production capture and streams.
// ABOUTME: Requires sonos-fixture, recognition off, SONOS_FIXTURE_URI and an absolute SONOS_FIXTURE_CA.

// Start one/two FLAC listeners during BASELINE and retain UART/API evidence through recovery.
// RECOVERED means all transport cycles passed; capture continuity still needs unchanged fault
// counters/generation and exact decoded sample counts. Use only the local fixture, never a speaker.

#[cfg(feature = "sonos-fixture")]
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};
#[cfg(feature = "sonos-fixture")]
use std::time::{Duration, Instant};

#[cfg(feature = "sonos-fixture")]
#[allow(dead_code)] // Only the build-time recognition flag is needed from this shared module.
#[path = "../src/config.rs"]
mod config;
#[cfg(feature = "sonos-fixture")]
#[allow(dead_code)] // Read the production clock without initializing another network stack.
#[path = "../src/net.rs"]
mod net;
#[cfg(feature = "sonos-fixture")]
mod recognizer {
    pub type SharedStore = std::sync::Arc<std::sync::Mutex<esp_shazam_core::store::SongStore>>;
}
#[cfg(feature = "sonos-fixture")]
#[allow(dead_code)] // Reuse the receive-only helper without launching a second controller.
#[path = "../src/sonos.rs"]
mod sonos;
#[cfg(feature = "sonos-fixture")]
#[path = "../src/threads.rs"]
mod threads;

#[cfg(feature = "sonos-fixture")]
const _: () = assert!(
    !config::CONFIG.shazam_enabled,
    "tls-capture requires shazam_enabled=false"
);

#[cfg(feature = "sonos-fixture")]
const CYCLES: u32 = 3;

#[cfg(feature = "sonos-fixture")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Probe {
    begin_us: u64,
    end_us: u64,
    current0: usize,
    audio_state: u8,
    cycle: u8,
    websocket_match: u8,
    audio_priority: u8,
}

#[cfg(feature = "sonos-fixture")]
struct Observation {
    epoch_us: u64,
    cycle: AtomicU32,
    recovery_done: AtomicBool,
}

#[cfg(feature = "sonos-fixture")]
fn observe(observation: Arc<Observation>, mut records: Vec<Probe>) {
    use esp_idf_svc::sys;
    let begin = observation.epoch_us + 35_000_000;
    let deadline = observation.epoch_us + 75_000_000;
    while micros() < begin {
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: production capture never returns (panic aborts), so a found audio
    // handle remains live. Other handles are compared as numbers, never dereferenced.
    let audio = unsafe { sys::xTaskGetHandle(c"audio".as_ptr()) };
    // SAFETY: null queries this observer; audio is the permanently live capture task.
    let observer_priority = unsafe { sys::uxTaskPriorityGet(std::ptr::null_mut()) };
    let initial_audio_priority = if audio.is_null() {
        u8::MAX
    } else {
        unsafe { sys::uxTaskPriorityGet(audio) as u8 }
    };
    let known = [
        ("audio", audio as usize),
        ("wifi", unsafe { sys::xTaskGetHandle(c"wifi".as_ptr()) }
            as usize),
        ("main", unsafe { sys::xTaskGetHandle(c"main".as_ptr()) }
            as usize),
        ("tiT", unsafe { sys::xTaskGetHandle(c"tiT".as_ptr()) }
            as usize),
    ];
    let mut used = 0;
    let mut overflow = false;
    loop {
        let begin_us = micros();
        if begin_us >= deadline {
            break;
        }
        if used == records.len() {
            overflow = true;
            break;
        }
        // SAFETY: core 0 exists. IDF returns an unprotected current TCB pointer;
        // retain only its numeric identity, with no name or other TCB dereference.
        let current0 = unsafe { sys::xTaskGetCurrentTaskHandleForCore(0) } as usize;
        // SAFETY: IDF searches task names under its kernel lock. Never dereference
        // the returned transient handle. The managed client defaults to this name.
        let websocket = unsafe { sys::xTaskGetHandle(c"websocket_task".as_ptr()) } as usize;
        // Numeric equality is not an atomic snapshot or proof of task lifetime:
        // lookup also searches terminating tasks, and handles/names may be reused.
        let websocket_match = u8::from(websocket != 0 && websocket == current0);
        // Lookup, state and priority each take the shared kernel lock. Probe duration
        // includes their waits; these observations need not describe the same instant.
        let (audio_state, audio_priority) = if audio.is_null() {
            (u8::MAX, u8::MAX)
        } else {
            // SAFETY: only the permanently live production capture handle is queried.
            unsafe {
                (
                    sys::eTaskGetState(audio) as u8,
                    sys::uxTaskPriorityGet(audio) as u8,
                )
            }
        };
        let cycle = observation.cycle.load(Ordering::Relaxed) as u8;
        records[used] = Probe {
            begin_us,
            end_us: micros(),
            current0,
            audio_state,
            cycle,
            websocket_match,
            audio_priority,
        };
        used += 1;
        // SAFETY: task context, one actual RTOS tick. No overdue catch-up burst.
        unsafe { sys::vTaskDelay(1) };
    }
    let stopped_us = micros();
    while micros() < observation.epoch_us + 90_000_000
        || (!observation.recovery_done.load(Ordering::Acquire)
            && micros() < observation.epoch_us + 120_000_000)
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: null selects this observer only; lower it before the UART dump.
    unsafe { sys::vTaskPrioritySet(std::ptr::null_mut(), 2) };
    // SAFETY: the fixed allocation is nonempty and live through this dump.
    let storage_psram = unsafe {
        sys::esp_ptr_external_ram(records.as_ptr().cast())
            && sys::esp_ptr_external_ram(records.as_ptr().add(records.len() - 1).cast())
    };
    let tick_us = 1_000_000 / u64::from(sys::configTICK_RATE_HZ);
    let dump_begin_us = micros();
    log::info!(
        "tls-observe: marker=BEGIN epoch_us={} window_begin_us={begin} window_end_us={deadline} stopped_us={stopped_us} dump_begin_us={dump_begin_us} tick_us={tick_us} observer_core=1 sample_priority={observer_priority} initial_audio_priority={initial_audio_priority} dump_priority=2 stack_bytes=4096 record_bytes={} capacity={} storage_bytes={} storage_psram={storage_psram} capture_found={} overflow={overflow} deadline_reached={} recovery_done={} unknown_state=255 running={} ready={} blocked={} suspended={} deleted={} websocket_lookup_name=websocket_task schema=index,cycle,begin_us,end_us,current0,audio_state,websocket_match,audio_priority",
        observation.epoch_us, std::mem::size_of::<Probe>(), records.len(),
        records.len() * std::mem::size_of::<Probe>(), !audio.is_null(), stopped_us >= deadline,
        observation.recovery_done.load(Ordering::Acquire),
        sys::eTaskState_eRunning, sys::eTaskState_eReady, sys::eTaskState_eBlocked,
        sys::eTaskState_eSuspended, sys::eTaskState_eDeleted,
    );
    for (name, handle) in known {
        log::info!("tls-observe: marker=TASK name={name} handle={handle}");
    }
    let mut max_probe_us = 0;
    let mut max_gap_us = 0;
    let mut gaps = 0;
    let mut previous = begin;
    for (index, record) in records[..used].iter().enumerate() {
        let gap_us = record.begin_us - previous;
        let probe_us = record.end_us - record.begin_us;
        max_gap_us = max_gap_us.max(gap_us);
        max_probe_us = max_probe_us.max(probe_us);
        gaps += usize::from(gap_us > 2 * tick_us);
        println!(
            "tls-observe: marker=SAMPLE row={index},{},{},{},{},{},{},{}",
            record.cycle,
            record.begin_us,
            record.end_us,
            record.current0,
            record.audio_state,
            record.websocket_match,
            record.audio_priority,
        );
        previous = record.begin_us;
        if index % 16 == 15 {
            // SAFETY: acquisition ended. UART polling must also let IDLE1 run.
            unsafe { sys::vTaskDelay(1) };
        }
    }
    log::info!(
        "tls-observe: marker=END records={used} overflow={overflow} max_probe_us={max_probe_us} max_gap_us={max_gap_us} gaps_over_two_ticks={gaps} start_late_us={} tail_us={} deadline_late_us={}",
        records[..used].first().map_or(stopped_us, |r| r.begin_us).saturating_sub(begin),
        deadline.saturating_sub(records[..used].last().map_or(begin, |r| r.end_us)),
        records[..used].last().map_or(0, |r| r.end_us.saturating_sub(deadline)),
    );
    // Current TCB identity does not identify ISR work. Gaps can include kernel-lock
    // waits, priority preemption or a stopped core-0 global tick, not just both cores.
}

#[cfg(feature = "sonos-fixture")]
fn experiment(observation: &Observation) -> anyhow::Result<()> {
    // Production owns Wi-Fi, NVS and SNTP; only wait for its already-running clock.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !net::clock_synced() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::ensure!(net::clock_synced(), "Fixture TLS requires a synced clock");

    let mut failures = 0;
    for cycle in 1..=CYCLES {
        observation.cycle.store(cycle, Ordering::Relaxed);
        marker("CONNECT_BEGIN", cycle);
        let started = Instant::now();
        let result = sonos::check_transport(
            env!("SONOS_FIXTURE_URI"),
            concat!(include_str!(env!("SONOS_FIXTURE_CA")), "\0").as_bytes(),
            Some(Duration::from_secs(2)),
        );
        match result {
            Ok(()) => log::info!(
                "tls-capture: marker=CYCLE_END cycle={cycle} ms={} elapsed_ms={} ok=true",
                millis(),
                started.elapsed().as_millis(),
            ),
            Err(error) => {
                failures += 1;
                log::error!(
                    "tls-capture: marker=CYCLE_END cycle={cycle} ms={} elapsed_ms={} ok=false error={error}",
                    millis(),
                    started.elapsed().as_millis(),
                );
            }
        }
        observation.cycle.store(0, Ordering::Relaxed);
        if cycle < CYCLES {
            std::thread::sleep(Duration::from_secs(10));
        }
    }
    // Preserve a capture-only recovery window even when a transport cycle failed.
    std::thread::sleep(Duration::from_secs(20));
    anyhow::ensure!(failures == 0, "{failures} of {CYCLES} TLS cycles failed");
    marker("RECOVERED", CYCLES);
    Ok(())
}

#[cfg(feature = "sonos-fixture")]
fn marker(name: &str, cycle: u32) {
    log::info!(
        "tls-capture: marker={name} cycle={cycle} ms={} cycles={CYCLES}",
        millis(),
    );
}

#[cfg(feature = "sonos-fixture")]
fn millis() -> u64 {
    micros() / 1_000
}

#[cfg(feature = "sonos-fixture")]
fn micros() -> u64 {
    // SAFETY: ESP-IDF starts this monotonic timer before entering main.
    unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 }
}

#[cfg(feature = "sonos-fixture")]
fn main() -> anyhow::Result<()> {
    // Fixed storage exists before production clocks and the measured baseline.
    let records = vec![Probe::default(); 4096];
    let observation = Arc::new(Observation {
        epoch_us: micros(),
        cycle: AtomicU32::new(0),
        recovery_done: AtomicBool::new(false),
    });
    let observer = observation.clone();
    threads::ThreadSpec {
        name: c"tls-observe",
        core: esp_idf_svc::hal::cpu::Core::Core1,
        priority: 11,
        stack_bytes: 4 * 1024,
    }
    .spawn(move || observe(observer, records))?;
    threads::ThreadSpec {
        name: c"tls-capture",
        core: esp_idf_svc::hal::cpu::Core::Core0,
        priority: 2,
        stack_bytes: 16 * 1024,
    }
    .spawn(move || {
        // Give production startup time to initialize logging, capture, Wi-Fi and SNTP.
        std::thread::sleep(Duration::from_secs(15));
        marker("BASELINE", 0);
        std::thread::sleep(Duration::from_secs(25));
        if let Err(error) = experiment(&observation) {
            log::error!("tls-capture: marker=FAILED ms={} error={error}", millis());
        }
        observation.recovery_done.store(true, Ordering::Release);
    })?;
    esp_shazam_firmware::run()
}

#[cfg(not(feature = "sonos-fixture"))]
fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    anyhow::bail!("tls-capture requires --features sonos-fixture and a local TLS fixture")
}
