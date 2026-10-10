// ABOUTME: Exercises capture restart failure handoff and metrics selection.
// ABOUTME: Uses the production module with a barrier-stalled real consumer.
#[path = "../../firmware/src/capture_failure_telemetry.rs"]
mod capture_failure_telemetry;

use capture_failure_telemetry::{ExportState, FailureKind, Handoff};
use std::sync::{Arc, Barrier};

#[test]
fn successful_restarts_do_not_report_failures() {
    let handoff = Handoff::new();
    let mut exports = ExportState::new();
    for kind in [FailureKind::InitialFlush, FailureKind::RecoveryRestart] {
        handoff.observe_restart_result(kind, Ok(()));
    }
    assert_eq!(exports.select(handoff.snapshot()), [None, None]);
}

#[test]
fn failures_are_isolated_signed_and_coalesced() {
    let handoff = Handoff::new();
    let mut exports = ExportState::new();
    handoff.observe_restart_result(FailureKind::InitialFlush, Err(i32::MIN));
    let selected = exports.select(handoff.snapshot());
    let initial = selected[0].unwrap();
    assert_eq!(initial.kind, FailureKind::InitialFlush);
    assert_eq!(
        (
            initial.failures_total,
            initial.failures_delta,
            initial.sampled_error_code
        ),
        (1, 1, i32::MIN)
    );
    assert!(selected[1].is_none());
    handoff.observe_restart_result(FailureKind::InitialFlush, Err(-7));
    handoff.observe_restart_result(FailureKind::InitialFlush, Err(i32::MAX));
    handoff.observe_restart_result(FailureKind::RecoveryRestart, Err(-42));
    let selected = exports.select(handoff.snapshot());
    let initial = selected[0].unwrap();
    assert_eq!(
        (
            initial.failures_total,
            initial.failures_delta,
            initial.sampled_error_code
        ),
        (3, 2, i32::MAX)
    );
    let recovery = selected[1].unwrap();
    assert_eq!(recovery.kind, FailureKind::RecoveryRestart);
    assert_eq!(
        (
            recovery.failures_total,
            recovery.failures_delta,
            recovery.sampled_error_code
        ),
        (1, 1, -42)
    );
    assert_eq!(exports.select(handoff.snapshot()), [None, None]);
}

#[test]
fn stalled_consumer_does_not_hold_up_recording() {
    let handoff = Arc::new(Handoff::new());
    let copied = Arc::new(Barrier::new(2));
    let resume = Arc::new(Barrier::new(2));
    let consumer = {
        let handoff = Arc::clone(&handoff);
        let copied = Arc::clone(&copied);
        let resume = Arc::clone(&resume);
        std::thread::spawn(move || {
            let mut exports = ExportState::new();
            let old = handoff.snapshot();
            copied.wait();
            resume.wait();
            assert_eq!(exports.select(old), [None, None]);
            let selected = exports.select(handoff.snapshot());
            let initial = selected[0].unwrap();
            let recovery = selected[1].unwrap();
            assert_eq!(
                (
                    initial.failures_total,
                    initial.failures_delta,
                    initial.sampled_error_code
                ),
                (128, 128, -127)
            );
            assert_eq!(
                (
                    recovery.failures_total,
                    recovery.failures_delta,
                    recovery.sampled_error_code
                ),
                (128, 128, 127)
            );
            assert_eq!(exports.select(handoff.snapshot()), [None, None]);
        })
    };
    copied.wait();
    for n in 0..128 {
        handoff.observe_restart_result(FailureKind::InitialFlush, Err(-n));
        handoff.observe_restart_result(FailureKind::RecoveryRestart, Err(n));
    }
    resume.wait();
    consumer.join().unwrap();
}

#[test]
fn selection_does_not_require_count_code_coherence() {
    use capture_failure_telemetry::FailureSample;
    let mut exports = ExportState::new();
    // A code can become visible before its count; it must not imply a failure.
    let samples = [
        FailureSample {
            failures_total: 0,
            sampled_error_code: -99,
        },
        FailureSample {
            failures_total: 0,
            sampled_error_code: 0,
        },
    ];
    assert_eq!(exports.select(samples), [None, None]);
    // A later count sample may carry any independently sampled numeric code.
    let samples = [
        FailureSample {
            failures_total: 1,
            sampled_error_code: -123,
        },
        FailureSample {
            failures_total: 0,
            sampled_error_code: 0,
        },
    ];
    assert_eq!(exports.select(samples)[0].unwrap().sampled_error_code, -123);
    let samples = [
        FailureSample {
            failures_total: 1,
            sampled_error_code: -456,
        },
        FailureSample {
            failures_total: 0,
            sampled_error_code: 0,
        },
    ];
    assert_eq!(exports.select(samples), [None, None]);
}
