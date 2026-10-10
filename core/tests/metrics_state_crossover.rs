// ABOUTME: Exercises the boot-timed observer gate with real microsecond deadlines.
// ABOUTME: Rejects skipped phases while allowing elapsed progression despite audio loss.

#[path = "../../firmware/src/metrics_state_crossover.rs"]
mod crossover;
use crossover::{Phase, Schedule};

#[test]
fn gate_changes_at_exact_boot_deadlines() {
    let mut schedule = Schedule::new();
    for (now, phase, enabled, deadline) in [
        (0, Phase::Settling, false, 0),
        (20_000_000, Phase::Off1, false, 20_000_000),
        (100_000_000, Phase::On, true, 100_000_000),
        (180_000_000, Phase::Off2, false, 180_000_000),
        (260_000_000, Phase::Finished, false, 260_000_000),
    ] {
        let transition = schedule.advance(now).unwrap();
        assert_eq!(transition.phase, phase);
        assert_eq!(transition.planned_us, deadline);
        assert!(!transition.skipped);
        assert_eq!(schedule.enabled(), enabled);
        assert!(schedule.advance(now).is_none());
    }
    assert!(schedule.advance(300_000_000).is_none());
    assert!(!schedule.enabled());
}

#[test]
fn last_microsecond_before_on_remains_off() {
    let mut schedule = Schedule::new();
    schedule.advance(0);
    schedule.advance(20_000_000);
    assert!(schedule.advance(99_999_999).is_none());
    assert!(!schedule.enabled());
    assert_eq!(schedule.advance(100_000_000).unwrap().phase, Phase::On);
    assert!(schedule.enabled());
}

#[test]
fn missing_first_or_intermediate_phase_is_reported_without_synthetic_records() {
    let mut late_start = Schedule::new();
    let first = late_start.advance(100_000_000).unwrap();
    assert_eq!(first.phase, Phase::On);
    assert!(first.skipped);
    assert!(late_start.advance(100_000_001).is_none());

    let mut schedule = Schedule::new();
    schedule.advance(0);
    let jumped = schedule.advance(180_000_000).unwrap();
    assert_eq!(jumped.phase, Phase::Off2);
    assert!(jumped.skipped);
    assert!(!schedule.enabled());
    assert!(!schedule.advance(260_000_000).unwrap().skipped);
}

#[test]
fn phase_records_use_the_frozen_sequence_names() {
    for (phase, sequence, name) in [
        (Phase::Settling, 0, "settling"),
        (Phase::Off1, 1, "OFF1"),
        (Phase::On, 2, "ON"),
        (Phase::Off2, 3, "OFF2"),
        (Phase::Finished, 4, "finished"),
    ] {
        assert_eq!(phase as u32, sequence);
        assert_eq!(phase.name(), name);
    }
}
