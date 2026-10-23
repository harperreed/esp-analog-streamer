// ABOUTME: Counts numeric task-state samples without inferring continuous occupancy.
// ABOUTME: Publishes independent atomic diagnostics from one local writer.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_states_and_rejects_changed_or_invalid_phases() {
        let mut counts = Counts::new();
        counts.record(2, 2, 1, 12, true);
        counts.record(2, 2, 2, 7, true);
        counts.record(2, 2, 99, 4, true);
        counts.record(2, 3, 1, 1, true);
        counts.record(99, 99, 1, 1, true);
        assert_eq!(counts.states[2], [1, 1, 1]);
        assert_eq!(counts.unassigned, 2);
        assert_eq!(counts.attempted, 5);
        assert_eq!(counts.max_query_us, 12);
        assert_eq!(counts.last_raw, 1);
    }

    #[test]
    fn unavailable_and_invalidated_queries_stay_visible() {
        let mut counts = Counts::new();
        counts.unavailable();
        counts.record(0, 0, 4, 1, true);
        assert!(counts.invalidated);
        assert_eq!(counts.unavailable, 1);
        assert_eq!(counts.unassigned, 1);
        let mut changed = Counts::new();
        changed.record(0, 0, 1, 2, false);
        assert!(changed.invalidated);
    }

    #[test]
    fn saturation_and_atomic_receipt_are_explicit() {
        let mut counts = Counts::new();
        counts.attempted = u32::MAX;
        counts.record(0, 0, 1, u64::from(u32::MAX) + 1, true);
        assert!(counts.saturated);
        let receipt = Receipt::new();
        receipt.publish(&counts);
        assert_eq!(receipt.counter(0, 0), 1);
        assert_eq!(receipt.attempted.load(Ordering::Relaxed), u32::MAX);
        assert!(receipt.saturated.load(Ordering::Relaxed));
    }

    #[test]
    fn phase_counters_and_saturation_are_independent_and_sticky() {
        let mut counts = Counts::new();
        for phase in 0..PHASES as u32 {
            counts.record(phase, phase, 2, 10, true);
        }
        assert!(counts.states.iter().all(|row| *row == [0, 1, 0]));
        counts.states[0][0] = u32::MAX;
        counts.record(0, 0, 1, 1, true);
        counts.record(1, 1, 0, 2, true);
        assert_eq!(counts.states[0][0], u32::MAX);
        assert_eq!(counts.states[1], [0, 1, 1]);
        assert!(counts.saturated);
        assert_eq!(counts.max_query_us, 10);
        counts.unavailable = u32::MAX;
        counts.unavailable();
        assert_eq!(counts.unavailable, u32::MAX);
        counts.unassigned = u32::MAX;
        counts.record(0, u32::MAX, 1, 0, true);
        assert_eq!(counts.unassigned, u32::MAX);
    }

    #[test]
    fn gate_uses_elapsed_time_without_catchup() {
        let mut gate = Gate::new(10);
        assert!(!gate.due(1_000_009, 1_000_000));
        assert!(gate.due(9_000_010, 1_000_000));
        assert!(!gate.due(9_000_010, 1_000_000));
        assert!(!gate.due(9_999_999, 1_000_000));
        assert!(gate.due(10_000_010, 1_000_000));
    }
}

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const PHASES: usize = 8;

#[repr(u32)]
#[derive(Clone, Copy)]
pub enum Phase {
    Init,
    HealthWarning,
    Capture,
    RestartExport,
    ListenerExport,
    EncoderExport,
    DiagnosticExport,
    Sleep,
}

/// A monotonic elapsed-time gate. Late work moves the origin to now, without catch-up.
pub struct Gate {
    origin_us: u64,
}

impl Gate {
    pub const fn new(origin_us: u64) -> Self {
        Self { origin_us }
    }

    pub fn due(&mut self, now_us: u64, period_us: u64) -> bool {
        if now_us.saturating_sub(self.origin_us) < period_us {
            return false;
        }
        self.origin_us = now_us;
        true
    }
}

/// One supervisor owns these counters. Numeric states follow pinned FreeRTOS bindings:
/// Ready=1, Blocked=2, Deleted=4; all other values remain observable as Other.
/// Equal phases allow ABA and do not prove continuous occupancy.
pub struct Counts {
    states: [[u32; 3]; PHASES],
    attempted: u32,
    unavailable: u32,
    unassigned: u32,
    last_raw: u32,
    max_query_us: u32,
    saturated: bool,
    pub invalidated: bool,
}

fn increment(value: &mut u32, saturated: &mut bool) {
    if *value == u32::MAX {
        *saturated = true;
    } else {
        *value += 1;
    }
}

impl Counts {
    pub const fn new() -> Self {
        Self {
            states: [[0; 3]; PHASES],
            attempted: 0,
            unavailable: 0,
            unassigned: 0,
            last_raw: u32::MAX,
            max_query_us: 0,
            saturated: false,
            invalidated: false,
        }
    }

    pub fn unavailable(&mut self) {
        increment(&mut self.attempted, &mut self.saturated);
        increment(&mut self.unavailable, &mut self.saturated);
    }

    pub fn record(
        &mut self,
        before: u32,
        after: u32,
        raw: u32,
        query_us: u64,
        identity_matches: bool,
    ) {
        increment(&mut self.attempted, &mut self.saturated);
        self.last_raw = raw;
        if query_us > u64::from(u32::MAX) {
            self.saturated = true;
        }
        self.max_query_us = self
            .max_query_us
            .max(query_us.min(u64::from(u32::MAX)) as u32);
        self.invalidated |= raw == 4 || !identity_matches;
        if self.invalidated || before != after || before >= PHASES as u32 {
            increment(&mut self.unassigned, &mut self.saturated);
            return;
        }
        let state = match raw {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        increment(
            &mut self.states[before as usize][state],
            &mut self.saturated,
        );
    }
}

impl Default for Counts {
    fn default() -> Self {
        Self::new()
    }
}

/// Independent relaxed atomic observations, never a coherent snapshot. Sums can differ
/// from attempted while publication or rendering is in progress. No reset-on-read.
pub struct Receipt {
    states: [[AtomicU32; 3]; PHASES],
    pub attempted: AtomicU32,
    pub unavailable: AtomicU32,
    pub unassigned: AtomicU32,
    pub last_raw: AtomicU32,
    pub max_query_us: AtomicU32,
    pub saturated: AtomicBool,
    pub invalidated: AtomicBool,
}

impl Receipt {
    pub const fn new() -> Self {
        Self {
            states: [const { [const { AtomicU32::new(0) }; 3] }; PHASES],
            attempted: AtomicU32::new(0),
            unavailable: AtomicU32::new(0),
            unassigned: AtomicU32::new(0),
            last_raw: AtomicU32::new(u32::MAX),
            max_query_us: AtomicU32::new(0),
            saturated: AtomicBool::new(false),
            invalidated: AtomicBool::new(false),
        }
    }

    /// Only the supervisor writer may publish.
    pub fn publish(&self, counts: &Counts) {
        for (dest, source) in self.states.iter().zip(counts.states.iter()) {
            for (dest, source) in dest.iter().zip(source.iter()) {
                dest.store(*source, Ordering::Relaxed);
            }
        }
        self.attempted.store(counts.attempted, Ordering::Relaxed);
        self.unavailable
            .store(counts.unavailable, Ordering::Relaxed);
        self.unassigned.store(counts.unassigned, Ordering::Relaxed);
        self.last_raw.store(counts.last_raw, Ordering::Relaxed);
        self.max_query_us
            .store(counts.max_query_us, Ordering::Relaxed);
        self.saturated.store(counts.saturated, Ordering::Relaxed);
        self.invalidated
            .store(counts.invalidated, Ordering::Relaxed);
    }

    pub fn counter(&self, phase: usize, state: usize) -> u32 {
        self.states
            .get(phase)
            .and_then(|row| row.get(state))
            .map_or(0, |counter| counter.load(Ordering::Relaxed))
    }
}

impl Default for Receipt {
    fn default() -> Self {
        Self::new()
    }
}
