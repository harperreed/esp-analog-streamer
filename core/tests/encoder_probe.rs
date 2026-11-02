// ABOUTME: Checks diagnostic window attribution and checked wall accounting.
// ABOUTME: Uses literal intervals to catch nested double counting and phase drift.
#![cfg(feature = "encoder-probe")]
#[path = "../../firmware/src/encoder_probe.rs"]
mod encoder_probe;
use encoder_probe::Probe;

#[test]
fn phases_discard_two_windows_each_and_finish_off() {
    let mut probe = Probe::default();
    probe.restart(true);
    for phase in ["OFF1", "ON", "OFF2"] {
        for interval in 1..=8 {
            assert_eq!(probe.enabled(), phase == "ON");
            let report = probe.report(100, 20, 30, 3, 2);
            assert_eq!(
                (report.phase, report.interval, report.retained),
                (phase, interval, interval > 2)
            );
            assert_eq!(report.completed, phase == "OFF2" && interval == 8);
            assert_eq!(report.residual_us, Some(45));
            assert_eq!(report.raw_remainder_us, Some(20));
        }
    }
    assert!(!probe.enabled());
    let report = probe.report(100, 0, 0, 0, 0);
    assert_eq!(report.phase, "COMPLETE");
    assert!(!report.completed);
}
#[test]
fn checked_remainders_are_independent_and_attempts_are_nested() {
    let mut probe = Probe::default();
    probe.restart(true);
    for _ in 0..8 {
        probe.report(0, 0, 0, 0, 0);
    }
    probe.attempt(true, 7);
    probe.attempt(false, 4);
    probe.push(5);
    probe.delay(9);
    let report = probe.report(100, 20, 30, 3, 2);
    assert_eq!(
        (report.raw_remainder_us, report.residual_us),
        (Some(9), Some(31))
    );
    assert_eq!(
        (
            report.success_us,
            report.rejected_us,
            report.push_us,
            report.delay_us,
            report.delay_calls
        ),
        (7, 4, 5, 9, 1)
    );
    probe.attempt(true, 21);
    let report = probe.report(100, 20, 30, 3, 2);
    assert_eq!(
        (report.raw_remainder_us, report.residual_us),
        (None, Some(45))
    );
    let report = probe.report(10, 20, 0, 0, 0);
    assert_eq!(
        (report.raw_remainder_us, report.residual_us),
        (Some(20), None)
    );
}
#[test]
fn invalidation_persists_until_receipts_and_fresh_baseline_restart() {
    let mut probe = Probe::default();
    probe.restart(true);
    probe.invalidate();
    assert!(!probe.report(100, 0, 0, 0, 0).valid);
    probe.restart(true);
    let report = probe.report(100, 0, 0, 0, 0);
    assert_eq!(
        (report.sequence, report.phase, report.interval, report.valid),
        (2, "OFF1", 1, true)
    );
    probe.invalidate();
    probe.restart(false);
    assert!(!probe.enabled());
    assert_eq!(probe.report(100, 0, 0, 0, 0).phase, "INACTIVE");
}

#[test]
fn overflow_marks_the_window_invalid_instead_of_wrapping() {
    let mut probe = Probe::default();
    probe.restart(true);
    for _ in 0..8 {
        probe.report(0, 0, 0, 0, 0);
    }
    probe.attempt(true, u64::MAX);
    probe.attempt(true, 1);
    let report = probe.report(u64::MAX, u64::MAX, 0, 0, 0);
    assert!(!report.valid);
}

#[test]
fn an_accounting_failure_prevents_a_valid_completion_for_the_whole_sequence() {
    let mut probe = Probe::default();
    probe.restart(true);
    assert!(!probe.report(0, 1, 0, 0, 0).valid);
    for _ in 1..23 {
        probe.report(100, 0, 0, 0, 0);
    }
    let final_window = probe.report(100, 0, 0, 0, 0);
    assert!(!final_window.valid);
    assert!(!final_window.completed);
}
