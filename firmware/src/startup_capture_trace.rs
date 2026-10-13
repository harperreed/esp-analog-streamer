// ABOUTME: Retains bounded startup phase and capture observations without blocking producers.
// ABOUTME: Atomic immutable slots let the existing metrics owner export delayed records.
use std::sync::atomic::{AtomicU32, Ordering};
// Word layout: kind, time lo/hi; ordinary observations carry callbacks, overflows,
// result code, sample end lo/hi. Kind 23 word 8 is the actual production baseline.
// Fault 30: previous/current overflow, callbacks, settling, last read begin lo/hi,
// end lo/hi, frames, result, staged frames, sample end lo/hi, sample begin lo/hi.
// The last read interval is historical; the bracket covers current fault samples.
// 31: words 8/9 are
// invalidation discard/new settling. 33: words 8/9/10 are read begin lo/hi/frames.
// Kinds 1/2 I2S, 3/4 codec, 5/6 spawn, 7/8 Wi-Fi construct, 9/10 Wi-Fi start;
// 20 capture entry, 21/22 initial flush, 23 baseline, 24/25 first read,
// 30 detection, 31/32 recovery restart, 33 next successful read.
pub const WORDS: usize = 24;
#[derive(Clone, Copy)]
pub struct Record {
    pub words: [u32; WORDS],
}
impl Record {
    pub fn new(kind: u32, time: u64) -> Self {
        let mut r = Self { words: [0; WORDS] };
        r.words[0] = kind;
        r.words[1] = time as u32;
        r.words[2] = (time >> 32) as u32;
        r
    }
    pub fn time(&self) -> u64 {
        u64::from(self.words[1]) | (u64::from(self.words[2]) << 32)
    }
}
struct Slot {
    ready: AtomicU32,
    words: [AtomicU32; WORDS],
}
impl Slot {
    const fn new() -> Self {
        Self {
            ready: AtomicU32::new(0),
            words: [const { AtomicU32::new(0) }; WORDS],
        }
    }
}
pub struct Bank<const N: usize> {
    slots: [Slot; N],
    attempted: AtomicU32,
    dropped: AtomicU32,
}
impl<const N: usize> Bank<N> {
    pub const fn new() -> Self {
        Self {
            slots: [const { Slot::new() }; N],
            attempted: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
        }
    }
    // Each bank has one producer; slots are never reused during the boot.
    pub fn publish(&self, record: Record) {
        let index = self.attempted.load(Ordering::Relaxed) as usize;
        if index >= N {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.attempted.store(index as u32 + 1, Ordering::Relaxed);
        let slot = &self.slots[index];
        for (word, value) in slot.words.iter().zip(record.words) {
            word.store(value, Ordering::Relaxed);
        }
        slot.ready.store(1, Ordering::Release);
    }
    pub fn read(&self, index: usize) -> Option<Record> {
        let slot = self.slots.get(index)?;
        if slot.ready.load(Ordering::Acquire) == 0 {
            return None;
        }
        Some(Record {
            words: std::array::from_fn(|i| slot.words[i].load(Ordering::Relaxed)),
        })
    }
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::Relaxed)
    }
}
pub struct Observer {
    baseline: u32,
    recoveries: u32,
    last: [u32; 6],
}
impl Observer {
    pub fn new() -> Self {
        Self {
            baseline: 0,
            recoveries: 0,
            last: [0; 6],
        }
    }
    pub fn baseline(&mut self, overflow: u32) {
        self.baseline = overflow;
    }
    pub fn read(&mut self, begin: u64, end: u64, frames: u32, code: i32) {
        if !self.complete() {
            self.last = [
                begin as u32,
                (begin >> 32) as u32,
                end as u32,
                (end >> 32) as u32,
                frames,
                code as u32,
            ];
        }
    }
    pub fn fault(
        &mut self,
        overflow: u32,
        now: u64,
        callbacks: u32,
        settling: u32,
    ) -> Option<Record> {
        if self.complete() || overflow == self.baseline {
            return None;
        }
        let mut r = Record::new(30, now);
        r.words[3..7].copy_from_slice(&[self.baseline, overflow, callbacks, settling]);
        r.words[7..13].copy_from_slice(&self.last);
        self.baseline = overflow;
        Some(r)
    }
    pub fn restarted(&mut self) {
        self.recoveries += 1;
    }
    pub fn complete(&self) -> bool {
        self.recoveries >= 2
    }
}
#[cfg(target_os = "espidf")]
mod device {
    use super::*;
    pub static MAIN: Bank<16> = Bank::new();
    pub static CAPTURE: Bank<16> = Bank::new();
    pub static COMPLETE: AtomicU32 = AtomicU32::new(0);
    pub fn now() -> u64 {
        unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 }
    }
    pub fn observation(kind: u32, code: i32) -> Record {
        let mut r = Record::new(kind, now());
        let counters = crate::i2s_capture::counters();
        r.words[3] = counters.callbacks;
        r.words[4] = counters.queue_overflows;
        r.words[5] = code as u32;
        let end = now();
        r.words[6] = end as u32;
        r.words[7] = (end >> 32) as u32;
        r
    }
    pub fn phase(kind: u32, code: i32) {
        MAIN.publish(observation(kind, code));
    }
    #[inline(never)]
    fn export_record(bank: u32, index: usize, r: Record) {
        log::info!("startup_trace: schema=1 bank={} index={} kind={} event_us={} independent_counters=true words={:?}", bank, index, r.words[0], r.time(), &r.words[3..]);
    }
    #[inline(never)]
    pub fn export(indices: &mut [usize; 2]) {
        for (id, bank) in [(0, &MAIN), (1, &CAPTURE)] {
            while let Some(r) = bank.read(indices[id]) {
                export_record(id as u32, indices[id], r);
                indices[id] += 1;
                std::thread::yield_now();
            }
        }
        log::info!("startup_trace_END: main_used={} capture_used={} main_capacity=16 capture_capacity=16 main_dropped={} capture_dropped={} capture_complete={} record_bytes={} bank_bytes={} fault_time_is_observation_upper_bound=true", indices[0], indices[1], MAIN.dropped(), CAPTURE.dropped(), COMPLETE.load(Ordering::Acquire), std::mem::size_of::<Record>(), std::mem::size_of::<Bank<16>>());
    }
}
#[cfg(target_os = "espidf")]
pub use device::*;
