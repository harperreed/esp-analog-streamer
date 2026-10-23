// ABOUTME: Pauses a diagnostic reader inside the real raw-ring copy without stopping capture.
// ABOUTME: Checks retained-copy equality, stale-copy rejection and fresh recovery with bounded waits.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys;
use esp_shazam_core::stream_ring::{Cursor, ReadError, StreamRing};

use crate::config::CONFIG;
use crate::threads::ThreadSpec;

mod events;
use events::Events;

const _: () = assert!(
    !CONFIG.shazam_enabled,
    "capture-publication requires shazam_enabled=false"
);
const FRAMES: usize = 240;
const STACK_BYTES: usize = 8 * 1024;
const WAIT_LIMIT: Duration = Duration::from_secs(2);
const RETRY_SLEEP: Duration = Duration::from_millis(10);
const EVENT_CAPACITY: usize = 16;

#[derive(Clone, Copy)]
enum Case {
    Short1,
    Short2,
    Long,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Short1 => "short-1",
            Self::Short2 => "short-2",
            Self::Long => "long",
        }
    }
}

#[derive(Clone, Copy)]
enum Event {
    Baseline {
        ms: u64,
        buffer_bytes: usize,
        reference_psram: bool,
        delayed_psram: bool,
    },
    PauseBegin {
        case: Case,
        ms: u64,
        requested_ms: u64,
    },
    PauseEnd {
        case: Case,
        ms: u64,
        elapsed_us: u64,
    },
    PauseResult {
        case: Case,
        ms: u64,
        verdict: std::result::Result<usize, ReadError>,
    },
    CaseOk {
        case: Case,
        ms: u64,
        frames: usize,
        paused_copy_accepted: bool,
    },
    Cleanup {
        ms: u64,
    },
}

type FixtureEvents = Events<Event, EVENT_CAPACITY>;

pub fn spawn(ring: Arc<StreamRing>) -> Result<()> {
    ThreadSpec {
        name: c"capture-reader",
        core: Core::Core1,
        priority: 2,
        stack_bytes: STACK_BYTES,
    }
    .spawn(move || {
        let mut events = FixtureEvents::new();
        let result = experiment(&ring, &mut events);
        // Cleanup precedes final reporting, even when a measurement failed.
        // This reader owns no publisher lock, slot pin or producer task handle.
        drop(ring);
        events.push(Event::Cleanup { ms: millis() });
        std::thread::sleep(Duration::from_secs(20));
        let recovered_ms = millis();
        // Synchronous UART output may block. All owned capture resources are
        // released before any formatting/output, including failure reporting.
        report(&events);
        match result {
            Ok(()) if !events.overflowed() => log::info!(
                "capture-publication: marker=RECOVERED retrospective=true ms={recovered_ms}"
            ),
            Ok(()) => log::error!(
                "capture-publication: marker=FAILED retrospective=true ms={recovered_ms} error=event_capacity_exhausted"
            ),
            Err(error) => log::error!(
                "capture-publication: marker=FAILED retrospective=true ms={recovered_ms} error={error:#}"
            ),
        }
    })
}

fn experiment(ring: &StreamRing, events: &mut FixtureEvents) -> Result<()> {
    ensure!(
        ring.capacity() == 96_000,
        "fixture requires 2 s raw history"
    );
    ensure!(
        ring.block_frames() == FRAMES,
        "fixture requires 5 ms blocks"
    );
    // Heap buffers exist before baseline. No 3,840-byte array lives on this task's stack.
    let mut reference = vec![0i32; 2 * FRAMES];
    let mut delayed = vec![0i32; 2 * FRAMES];
    std::thread::sleep(Duration::from_secs(15));
    events.push(Event::Baseline {
        ms: millis(),
        buffer_bytes: (reference.capacity() + delayed.capacity()) * std::mem::size_of::<i32>(),
        reference_psram: in_psram(&reference),
        delayed_psram: in_psram(&delayed),
    });
    std::thread::sleep(Duration::from_secs(25));
    run_case(
        ring,
        Case::Short1,
        Duration::from_millis(200),
        false,
        &mut reference,
        &mut delayed,
        events,
    )
    .context("short-1")?;
    std::thread::sleep(Duration::from_secs(10));
    run_case(
        ring,
        Case::Short2,
        Duration::from_millis(200),
        false,
        &mut reference,
        &mut delayed,
        events,
    )
    .context("short-2")?;
    std::thread::sleep(Duration::from_secs(10));
    run_case(
        ring,
        Case::Long,
        Duration::from_millis(2_500),
        true,
        &mut reference,
        &mut delayed,
        events,
    )
    .context("long")?;
    Ok(())
}

fn run_case(
    ring: &StreamRing,
    case: Case,
    pause: Duration,
    stale: bool,
    reference: &mut [i32],
    delayed: &mut [i32],
    events: &mut FixtureEvents,
) -> Result<()> {
    let start = retry(|| ring.cursor_now().map(Some))?;
    let mut reference_cursor = start;
    let frames = read_fresh(ring, &mut reference_cursor, reference)?;
    ensure!(frames == FRAMES, "reference is not one complete block");
    let mut cursor = start;
    let verdict = delayed_copy(ring, &mut cursor, delayed, case, pause, events)?;
    if verdict.is_err() {
        ensure!(cursor == start, "rejected copy advanced its cursor");
    }
    if stale {
        ensure!(verdict.is_err(), "copy beyond raw history was accepted");
        // Retry is legitimate during a concurrent publication, but this cursor
        // must then become an explicit Overrun, never provide stale scratch.
        let deadline = Instant::now() + WAIT_LIMIT;
        loop {
            match ring.read(&mut cursor, delayed) {
                Err(ReadError::Overrun) => break,
                Err(ReadError::Retry) => wait_before(deadline)?,
                Ok(_) => bail!("stale cursor returned usable audio or caught-up status"),
            }
        }
        ensure!(cursor == start, "stale read advanced its cursor");
        cursor = retry(|| ring.cursor_now().map(Some))?;
        let fresh = read_fresh(ring, &mut cursor, delayed)?;
        ensure!(fresh == FRAMES, "fresh recovery is not one complete block");
        events.push(Event::CaseOk {
            case,
            ms: millis(),
            frames: fresh,
            paused_copy_accepted: false,
        });
    } else {
        let accepted = match verdict {
            Ok(n) => n,
            Err(ReadError::Retry) => read_fresh(ring, &mut cursor, delayed)?,
            Err(ReadError::Overrun) => bail!("short pause lost retained raw audio"),
        };
        ensure!(accepted == frames, "retained read changed its frame count");
        ensure!(
            cursor == reference_cursor,
            "retained read advanced incorrectly"
        );
        ensure!(
            reference[..2 * frames] == delayed[..2 * frames],
            "retained block differs from real reference"
        );
        events.push(Event::CaseOk {
            case,
            ms: millis(),
            frames,
            paused_copy_accepted: verdict.is_ok(),
        });
    }
    Ok(())
}

fn delayed_copy(
    ring: &StreamRing,
    cursor: &mut Cursor,
    out: &mut [i32],
    case: Case,
    pause: Duration,
    events: &mut FixtureEvents,
) -> Result<std::result::Result<usize, ReadError>> {
    let deadline = Instant::now() + WAIT_LIMIT;
    loop {
        let mut entered = false;
        let verdict = ring.read_with_copy_hook(cursor, out, || {
            entered = true;
            let started = Instant::now();
            events.push(Event::PauseBegin {
                case,
                ms: millis(),
                requested_ms: pause.as_millis() as u64,
            });
            std::thread::sleep(pause);
            events.push(Event::PauseEnd {
                case,
                ms: millis(),
                elapsed_us: started.elapsed().as_micros() as u64,
            });
        });
        if entered {
            events.push(Event::PauseResult {
                case,
                ms: millis(),
                verdict,
            });
            return Ok(verdict);
        }
        match verdict {
            Ok(0) | Err(ReadError::Retry) => wait_before(deadline)?,
            Err(ReadError::Overrun) => bail!("reference expired before entering actual copy"),
            Ok(_) => bail!("actual copy did not invoke its diagnostic hook"),
        }
    }
}

fn read_fresh(ring: &StreamRing, cursor: &mut Cursor, out: &mut [i32]) -> Result<usize> {
    retry(|| ring.read(cursor, out).map(|n| (n > 0).then_some(n)))
}

fn retry<T>(mut operation: impl FnMut() -> std::result::Result<Option<T>, ReadError>) -> Result<T> {
    let deadline = Instant::now() + WAIT_LIMIT;
    loop {
        match operation() {
            Ok(Some(value)) => return Ok(value),
            Ok(None) | Err(ReadError::Retry) => wait_before(deadline)?,
            Err(ReadError::Overrun) => bail!("raw reader overran history"),
        }
    }
}

fn wait_before(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "raw reader exceeded 2 s wait budget"
    );
    std::thread::sleep(RETRY_SLEEP);
    Ok(())
}

fn in_psram(samples: &[i32]) -> bool {
    // SAFETY: both pointers belong to this nonempty live diagnostic allocation.
    unsafe {
        sys::esp_ptr_external_ram(samples.as_ptr().cast())
            && sys::esp_ptr_external_ram(samples.as_ptr().add(samples.len() - 1).cast())
    }
}

fn report(events: &FixtureEvents) {
    log::info!(
        "capture-publication: marker=REPORT retrospective=true event_capacity={EVENT_CAPACITY} event_storage_bytes={} overflowed={}",
        std::mem::size_of::<FixtureEvents>(), events.overflowed(),
    );
    for (index, event) in events.iter().enumerate() {
        match *event {
            Event::Baseline { ms, buffer_bytes, reference_psram, delayed_psram } => log::info!(
                "capture-publication: marker=BASELINE retrospective=true ms={ms} core=1 priority=2 stack_bytes={STACK_BYTES} buffer_bytes={buffer_bytes} reference_psram={reference_psram} delayed_psram={delayed_psram} capacity_frames=96000 block_frames={FRAMES} short_pause_ms=200 long_pause_ms=2500"
            ),
            Event::PauseBegin { case, ms, requested_ms } => log::info!(
                "capture-publication: marker=PAUSE_BEGIN retrospective=true case={} ms={ms} requested_ms={requested_ms}", case.name()
            ),
            Event::PauseEnd { case, ms, elapsed_us } => log::info!(
                "capture-publication: marker=PAUSE_END retrospective=true case={} ms={ms} elapsed_us={elapsed_us}", case.name()
            ),
            Event::PauseResult { case, ms, verdict } => log::info!(
                "capture-publication: marker=PAUSE_RESULT retrospective=true case={} ms={ms} verdict={verdict:?}", case.name()
            ),
            Event::CaseOk { case: Case::Long, ms, frames, .. } => log::info!(
                "capture-publication: marker=CASE_OK retrospective=true case=long ms={ms} stale_rejected=true fresh_frames={frames}"
            ),
            Event::CaseOk { case, ms, frames, paused_copy_accepted } => log::info!(
                "capture-publication: marker=CASE_OK retrospective=true case={} ms={ms} coherent=true frames={frames} paused_copy_accepted={paused_copy_accepted}", case.name()
            ),
            Event::Cleanup { ms } => log::info!(
                "capture-publication: marker=CLEANUP retrospective=true ms={ms}"
            ),
        }
        // UART polling can starve idle even at priority 2; reporting yields.
        if (index + 1) % 4 == 0 {
            std::thread::sleep(RETRY_SLEEP);
        }
    }
}

fn millis() -> u64 {
    // SAFETY: ESP-IDF starts this monotonic timer before the firmware entry point.
    unsafe { sys::esp_timer_get_time() as u64 / 1_000 }
}
