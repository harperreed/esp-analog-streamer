// ABOUTME: Bounds frame-write-state observations to the first two compressed listeners.
// ABOUTME: Publishes one immutable receipt; metrics never receives a live task handle.
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Exemplar {
    pub sequence: u32,
    pub start_us: u32,
    pub end_us: u32,
    pub state: u32,
    pub age_us: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotReceipt {
    pub selected: bool,
    pub live: bool,
    pub invalid: bool,
    pub address: [u8; 4],
    pub port: u32,
    pub registered_us: u32,
    pub attempted: u32,
    pub inactive: u32,
    pub unregistered: u32,
    pub unavailable: u32,
    pub states: [u32; 6],
    pub aged_states: [u32; 6],
    pub query_total_us: u32,
    pub query_max_us: u32,
    pub exemplar: Exemplar,
    pub replaced: u32,
    pub not_retained: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Receipt {
    pub slots: [SlotReceipt; 2],
    pub selected: u32,
    pub unselected: u32,
    pub opportunities: u32,
    pub max_gap_us: u32,
    pub skipped: u32,
    pub saturated: bool,
    pub frozen_us: u32,
    pub late_us: u32,
    pub early: bool,
    pub clock_invalid: bool,
}
struct Slot {
    reserved: AtomicBool,
    ready: AtomicBool,
    handle: AtomicUsize,
    address: AtomicU32,
    port: AtomicU32,
    registered: AtomicU32,
    sequence: AtomicU32,
    active: AtomicU32,
    start: AtomicU32,
    invalid: AtomicBool,
}
impl Slot {
    const fn new() -> Self {
        Self {
            reserved: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            handle: AtomicUsize::new(0),
            address: AtomicU32::new(0),
            port: AtomicU32::new(0),
            registered: AtomicU32::new(0),
            sequence: AtomicU32::new(0),
            active: AtomicU32::new(0),
            start: AtomicU32::new(0),
            invalid: AtomicBool::new(false),
        }
    }
}
pub struct Diagnostic {
    slots: [Slot; 2],
    terminal: AtomicBool,
    published: AtomicBool,
    unselected: AtomicU32,
    receipt: UnsafeCell<Receipt>,
}
// SAFETY: slots are atomic. Only the supervisor writes receipt, once, before the
// release published store. Readers acquire that store; receipt is never changed again.
unsafe impl Sync for Diagnostic {}
impl Diagnostic {
    pub const fn new() -> Self {
        Self {
            slots: [Slot::new(), Slot::new()],
            terminal: AtomicBool::new(false),
            published: AtomicBool::new(false),
            unselected: AtomicU32::new(0),
            receipt: UnsafeCell::new(Receipt {
                slots: [EMPTY_SLOT; 2],
                selected: 0,
                unselected: 0,
                opportunities: 0,
                max_gap_us: 0,
                skipped: 0,
                saturated: false,
                frozen_us: 0,
                late_us: 0,
                early: false,
                clock_invalid: false,
            }),
        }
    }
    pub fn register(
        &self,
        now: u32,
        handle: usize,
        address: [u8; 4],
        port: u32,
    ) -> Option<Owner<'_>> {
        if self.terminal.load(Ordering::Acquire) || handle == 0 {
            return None;
        }
        for (index, slot) in self.slots.iter().enumerate() {
            if slot
                .reserved
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                slot.address
                    .store(u32::from_be_bytes(address), Ordering::Relaxed);
                slot.port.store(port, Ordering::Relaxed);
                slot.registered.store(now, Ordering::Relaxed);
                slot.handle.store(handle, Ordering::Relaxed);
                slot.ready.store(true, Ordering::Release);
                return Some(Owner {
                    diagnostic: self,
                    index,
                });
            }
        }
        let _ = self
            .unselected
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_add(1))
            });
        None
    }
    pub fn receipt(&self) -> Option<&Receipt> {
        if self.published.load(Ordering::Acquire) {
            Some(unsafe { &*self.receipt.get() })
        } else {
            None
        }
    }
}
const EMPTY_SLOT: SlotReceipt = SlotReceipt {
    selected: false,
    live: false,
    invalid: false,
    address: [0; 4],
    port: 0,
    registered_us: 0,
    attempted: 0,
    inactive: 0,
    unregistered: 0,
    unavailable: 0,
    states: [0; 6],
    aged_states: [0; 6],
    query_total_us: 0,
    query_max_us: 0,
    exemplar: Exemplar {
        sequence: 0,
        start_us: 0,
        end_us: 0,
        state: 0,
        age_us: 0,
    },
    replaced: 0,
    not_retained: 0,
};
pub struct Owner<'a> {
    diagnostic: &'a Diagnostic,
    index: usize,
}
impl Owner<'_> {
    pub fn begin(&mut self, now: u32) {
        if self.diagnostic.terminal.load(Ordering::Acquire) {
            return;
        }
        let s = &self.diagnostic.slots[self.index];
        let seq = s.sequence.load(Ordering::Relaxed);
        if seq == u32::MAX {
            s.invalid.store(true, Ordering::Release);
            self.end();
            return;
        }
        s.sequence.store(seq + 1, Ordering::Relaxed);
        s.start.store(now, Ordering::Relaxed);
        s.active.store(seq + 1, Ordering::Release);
    }
    pub fn end(&mut self) {
        self.diagnostic.slots[self.index]
            .active
            .store(0, Ordering::Release);
    }
}
impl Drop for Owner<'_> {
    fn drop(&mut self) {
        self.end();
        self.diagnostic.slots[self.index]
            .handle
            .store(0, Ordering::Release);
    }
}
pub struct Sampler {
    launched: u32,
    last: Option<u32>,
    counts: Receipt,
}
impl Sampler {
    pub fn new(launched: u32) -> Self {
        Self {
            launched,
            last: None,
            counts: Receipt::default(),
        }
    }
    pub fn opportunity(
        &mut self,
        d: &Diagnostic,
        now: u32,
        mut query: impl FnMut(usize) -> (u32, u32, u32),
    ) -> bool {
        if d.terminal.load(Ordering::Acquire) {
            return true;
        }
        let c = &mut self.counts;
        let empty_deadline = self.launched.saturating_add(30_000_000);
        // A registration after the empty cutoff cannot revive acquisition while
        // the supervisor is late. An eligible first owner opens the full window.
        let first = d
            .slots
            .iter()
            .filter(|slot| slot.ready.load(Ordering::Acquire))
            .map(|slot| slot.registered.load(Ordering::Relaxed))
            .min()
            .filter(|registered| *registered <= empty_deadline);
        let mut retired = 0;
        for (i, s) in d.slots.iter().enumerate() {
            if first.is_some() && s.ready.load(Ordering::Acquire) {
                let r = &mut c.slots[i];
                r.selected = true;
                r.registered_us = s.registered.load(Ordering::Relaxed);
                r.address = s.address.load(Ordering::Relaxed).to_be_bytes();
                r.port = s.port.load(Ordering::Relaxed);
                r.live = s.handle.load(Ordering::Acquire) != 0;
                r.invalid = s.invalid.load(Ordering::Acquire);
                if !r.live {
                    retired += 1;
                }
            }
        }
        c.selected = c.slots.iter().filter(|s| s.selected).count() as u32;
        let deadline = first.map_or(empty_deadline, |t| t.saturating_add(10_000_000));
        c.clock_invalid = self.launched > u32::MAX - 30_000_000
            || now < self.launched
            || self.last.is_some_and(|last| now < last);
        if (c.selected == 2 && retired == 2) || now >= deadline || c.clock_invalid {
            if d.terminal
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return true;
            }
            // Retirement observation at/after cutoff cannot establish early completion.
            c.early = c.selected == 2 && retired == 2 && now < deadline && !c.clock_invalid;
            c.frozen_us = now;
            c.late_us = now.saturating_sub(deadline);
            c.unselected = d.unselected.load(Ordering::Relaxed);
            c.saturated |= c.unselected == u32::MAX;
            // SAFETY: the sole supervisor publishes once. Terminal prevents another write;
            // release/acquire published transfers the complete copied block to metrics.
            unsafe {
                *d.receipt.get() = *c;
            }
            d.published.store(true, Ordering::Release);
            return true;
        }
        add(&mut c.opportunities, 1, &mut c.saturated);
        if let Some(last) = self.last {
            let gap = now.saturating_sub(last);
            c.max_gap_us = c.max_gap_us.max(gap);
            add(
                &mut c.skipped,
                (gap / 100_000).saturating_sub(1),
                &mut c.saturated,
            );
        }
        self.last = Some(now);
        for (i, s) in d.slots.iter().enumerate() {
            let r = &mut c.slots[i];
            add(&mut r.attempted, 1, &mut c.saturated);
            let handle = s.handle.load(Ordering::Acquire);
            let token = s.active.load(Ordering::Acquire);
            if !r.selected {
                add(&mut r.unregistered, 1, &mut c.saturated);
                continue;
            }
            if handle == 0 || token == 0 {
                add(&mut r.inactive, 1, &mut c.saturated);
                continue;
            }
            if r.invalid {
                add(&mut r.unavailable, 1, &mut c.saturated);
                continue;
            }
            let start = s.start.load(Ordering::Relaxed);
            let (state, query_start, end) = query(handle);
            let elapsed = end.saturating_sub(query_start);
            add(&mut r.query_total_us, elapsed, &mut c.saturated);
            r.query_max_us = r.query_max_us.max(elapsed);
            if handle != s.handle.load(Ordering::Acquire)
                || token != s.active.load(Ordering::Acquire)
                || !s.ready.load(Ordering::Acquire)
            {
                add(&mut r.unavailable, 1, &mut c.saturated);
                continue;
            }
            let index = if state < 5 { state as usize } else { 5 };
            if index == 4 {
                r.invalid = true;
                s.invalid.store(true, Ordering::Release);
                add(&mut r.unavailable, 1, &mut c.saturated);
            }
            add(&mut r.states[index], 1, &mut c.saturated);
            let age = query_start.saturating_sub(start);
            if age >= 100_000 {
                add(&mut r.aged_states[index], 1, &mut c.saturated);
            }
            // Deleted is an invariant failure, never a valid sampled exemplar.
            if index == 4 {
                continue;
            }
            if r.exemplar.sequence == 0 || age > r.exemplar.age_us {
                if r.exemplar.sequence != 0 {
                    add(&mut r.replaced, 1, &mut c.saturated);
                }
                r.exemplar = Exemplar {
                    sequence: token,
                    start_us: query_start,
                    end_us: end,
                    state,
                    age_us: age,
                };
            } else {
                add(&mut r.not_retained, 1, &mut c.saturated);
            }
        }
        false
    }
}
fn add(value: &mut u32, amount: u32, saturated: &mut bool) {
    match value.checked_add(amount) {
        Some(v) => *value = v,
        None => {
            *value = u32::MAX;
            *saturated = true;
        }
    }
}

#[cfg(target_os = "espidf")]
pub static DIAGNOSTIC: Diagnostic = Diagnostic::new();
#[cfg(target_os = "espidf")]
pub fn now() -> u32 {
    unsafe { esp_idf_svc::sys::esp_timer_get_time() as u32 }
}
#[cfg(target_os = "espidf")]
pub fn register_current(peer: Option<std::net::SocketAddr>) -> Option<Owner<'static>> {
    let (address, port) = match peer {
        Some(std::net::SocketAddr::V4(a)) => (a.ip().octets(), u32::from(a.port())),
        _ => ([0; 4], 0),
    };
    // SAFETY: current handle belongs to this compressed listener scope. Its guard
    // clears it before departure and std pthread teardown. No task lookup occurs.
    DIAGNOSTIC.register(
        now(),
        unsafe { esp_idf_svc::sys::xTaskGetCurrentTaskHandle() } as usize,
        address,
        port,
    )
}
#[cfg(target_os = "espidf")]
pub fn observe(sampler: &mut Sampler) {
    sampler.opportunity(&DIAGNOSTIC, now(), |handle| {
        let started = now();
        // SAFETY: supervisor and listener are pinned core1, priorities11 and7. This
        // synchronous kernel query does not yield/block; owner cannot exit on this core
        // while supervisor runs. Drop clears before Rust/pthread detached deletion.
        let state =
            unsafe { esp_idf_svc::sys::eTaskGetState(handle as esp_idf_svc::sys::TaskHandle_t) };
        (state as u32, started, now())
    });
}
#[cfg(target_os = "espidf")]
#[inline(never)]
pub fn export(receipt: &Receipt) {
    export_begin(receipt);
    yield_export();
    for (index, slot) in receipt.slots.iter().enumerate() {
        export_slot(index + 1, slot);
        yield_export();
        export_row(index + 1, false, &slot.states);
        yield_export();
        export_row(index + 1, true, &slot.aged_states);
        yield_export();
        export_exemplar(index + 1, &slot.exemplar);
        yield_export();
    }
    log::info!("frame-write-state: complete schema=1 slots=2 rows=4 exemplars=2");
}
#[cfg(target_os = "espidf")]
fn yield_export() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}
#[cfg(target_os = "espidf")]
#[inline(never)]
fn export_begin(r: &Receipt) {
    let live_owners = r.slots.iter().filter(|slot| slot.live).count();
    let incomplete = r.selected != 2 || live_owners != 0 || r.clock_invalid;
    log::info!("frame-write-state: begin schema=1 capacity=2 selected={} unselected={} opportunities={} skipped={} max_gap_us={} frozen_us={} cutoff_late_us={} early={} expired={} incomplete={} live_owners={} saturated={} clock_invalid={} receipt_bytes={} diagnostic_bytes={}",r.selected,r.unselected,r.opportunities,r.skipped,r.max_gap_us,r.frozen_us,r.late_us,r.early,!r.early && !r.clock_invalid,incomplete,live_owners,r.saturated,r.clock_invalid,std::mem::size_of::<Receipt>(),std::mem::size_of::<Diagnostic>());
}
#[cfg(target_os = "espidf")]
#[inline(never)]
fn export_slot(id: usize, s: &SlotReceipt) {
    log::info!("frame-write-state: slot schema=1 id={} selected={} live={} invalid={} peer={}.{}.{}.{}:{} registered_us={} attempted={} inactive={} unregistered={} unavailable={} query_total_us={} query_max_us={} replaced={} not_retained={}",id,s.selected,s.live,s.invalid,s.address[0],s.address[1],s.address[2],s.address[3],s.port,s.registered_us,s.attempted,s.inactive,s.unregistered,s.unavailable,s.query_total_us,s.query_max_us,s.replaced,s.not_retained);
}
#[cfg(target_os = "espidf")]
#[inline(never)]
fn export_row(id: usize, aged: bool, row: &[u32; 6]) {
    let valid = row
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 4)
        .map(|(_, value)| u64::from(*value))
        .sum::<u64>();
    log::info!("frame-write-state: row schema=1 id={} age_ge_100ms={} valid={} running={} ready={} blocked={} suspended={} deleted={} other={}",id,aged,valid,row[0],row[1],row[2],row[3],row[4],row[5]);
}
#[cfg(target_os = "espidf")]
#[inline(never)]
fn export_exemplar(id: usize, e: &Exemplar) {
    log::info!("frame-write-state: exemplar schema=1 id={} valid={} sequence={} query_start_us={} query_end_us={} state={} age_us={}",id,e.sequence!=0,e.sequence,e.start_us,e.end_us,e.state,e.age_us);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequence_overflow_disables_without_reusing_zero() {
        let d = Diagnostic::new();
        let mut owner = d.register(1, 1, [0; 4], 1).unwrap();
        d.slots[0].sequence.store(u32::MAX, Ordering::Relaxed);
        owner.begin(2);
        assert!(d.slots[0].invalid.load(Ordering::Acquire));
        assert_eq!(d.slots[0].active.load(Ordering::Acquire), 0);
        assert_eq!(d.slots[0].sequence.load(Ordering::Relaxed), u32::MAX);
    }
    #[test]
    fn counters_saturate_with_sticky_flag() {
        let mut value = u32::MAX - 1;
        let mut saturated = false;
        add(&mut value, 2, &mut saturated);
        assert_eq!(value, u32::MAX);
        assert!(saturated);
        add(&mut value, 0, &mut saturated);
        assert!(saturated);
        let d = Diagnostic::new();
        let mut sampler = Sampler::new(0);
        sampler.counts.opportunities = u32::MAX;
        sampler.opportunity(&d, 1, |_| unreachable!());
        sampler.opportunity(&d, 30_000_000, |_| unreachable!());
        assert!(d.receipt().unwrap().saturated);
    }
    #[test]
    fn tied_and_younger_exemplars_are_not_retained() {
        let d = Diagnostic::new();
        let mut owner = d.register(1, 1, [0; 4], 1).unwrap();
        owner.begin(2);
        let mut sampler = Sampler::new(0);
        sampler.opportunity(&d, 100002, |_| (1, 100002, 100003));
        owner.end();
        owner.begin(100004);
        sampler.opportunity(&d, 100005, |_| (2, 100005, 100006));
        owner.end();
        owner.begin(100007);
        sampler.opportunity(&d, 200007, |_| (3, 200007, 200008));
        sampler.opportunity(&d, 10_000_001, |_| unreachable!());
        let r = d.receipt().unwrap();
        assert_eq!(r.slots[0].not_retained, 2);
        assert_eq!(r.slots[0].exemplar.sequence, 1);
    }
    #[test]
    fn field_layout_uses_only_native_atomic_types() {
        assert_eq!(std::mem::size_of::<AtomicU32>(), 4);
        assert_eq!(std::mem::size_of::<Exemplar>(), 20);
        assert_eq!(std::mem::size_of::<SlotReceipt>(), 116);
        // Bound stack/static consumption; this Rust block is not a wire layout.
        assert!(std::mem::size_of::<Receipt>() <= 288);
        assert!(std::mem::size_of::<Diagnostic>() <= 400);
        eprintln!(
            "layout host: slot={} receipt={} diagnostic={} sampler={}",
            std::mem::size_of::<Slot>(),
            std::mem::size_of::<Receipt>(),
            std::mem::size_of::<Diagnostic>(),
            std::mem::size_of::<Sampler>()
        );
    }
}
