// ABOUTME: Hands numeric capture restart failures to the metrics owner.
// ABOUTME: Counts wrap modulo 2^32; sampled codes are independent observations.
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    InitialFlush,
    RecoveryRestart,
}

struct Counter {
    failures: AtomicU32,
    sampled_error_code: AtomicI32,
}

impl Counter {
    const fn new() -> Self {
        Self {
            failures: AtomicU32::new(0),
            sampled_error_code: AtomicI32::new(0),
        }
    }

    fn snapshot(&self) -> FailureSample {
        FailureSample {
            failures_total: self.failures.load(Ordering::Relaxed),
            sampled_error_code: self.sampled_error_code.load(Ordering::Relaxed),
        }
    }
}

pub struct Handoff {
    initial_flush: Counter,
    recovery_restart: Counter,
}

impl Handoff {
    pub const fn new() -> Self {
        Self {
            initial_flush: Counter::new(),
            recovery_restart: Counter::new(),
        }
    }

    pub fn observe_restart_result(&self, kind: FailureKind, result: Result<(), i32>) {
        if let Err(code) = result {
            let counter = match kind {
                FailureKind::InitialFlush => &self.initial_flush,
                FailureKind::RecoveryRestart => &self.recovery_restart,
            };
            counter.sampled_error_code.store(code, Ordering::Relaxed);
            counter.failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Returns InitialFlush then RecoveryRestart.
    /// Counts and codes are independent atomic samples, never coherent tuples.
    pub fn snapshot(&self) -> [FailureSample; 2] {
        [
            self.initial_flush.snapshot(),
            self.recovery_restart.snapshot(),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FailureSample {
    pub failures_total: u32,
    pub sampled_error_code: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub kind: FailureKind,
    pub failures_total: u32,
    pub failures_delta: u32,
    pub sampled_error_code: i32,
}

/// Metrics-owned selection state; selection advances counts before log delivery.
pub struct ExportState {
    previous_counts: [u32; 2],
}

impl ExportState {
    pub const fn new() -> Self {
        Self {
            previous_counts: [0; 2],
        }
    }

    /// Equal samples can hide a full count wrap; intermediate codes are lost.
    pub fn select(&mut self, samples: [FailureSample; 2]) -> [Option<Summary>; 2] {
        let kinds = [FailureKind::InitialFlush, FailureKind::RecoveryRestart];
        core::array::from_fn(|index| {
            let sample = samples[index];
            let previous = &mut self.previous_counts[index];
            if sample.failures_total == *previous {
                return None;
            }
            let failures_delta = sample.failures_total.wrapping_sub(*previous);
            *previous = sample.failures_total;
            Some(Summary {
                kind: kinds[index],
                failures_total: sample.failures_total,
                failures_delta,
                sampled_error_code: sample.sampled_error_code,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_count_exports_wrapping_delta() {
        let handoff = Handoff::new();
        handoff
            .initial_flush
            .failures
            .store(u32::MAX - 1, Ordering::Relaxed);
        let mut exports = ExportState::new();
        assert_eq!(
            exports.select(handoff.snapshot())[0]
                .unwrap()
                .failures_delta,
            u32::MAX - 1
        );
        handoff.observe_restart_result(FailureKind::InitialFlush, Err(-1));
        handoff.observe_restart_result(FailureKind::InitialFlush, Err(-2));
        let report = exports.select(handoff.snapshot())[0].unwrap();
        assert_eq!(
            (
                report.failures_total,
                report.failures_delta,
                report.sampled_error_code
            ),
            (0, 2, -2)
        );
        assert_eq!(exports.select(handoff.snapshot()), [None, None]);
    }
}
