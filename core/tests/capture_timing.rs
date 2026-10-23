// ABOUTME: Checks the capture service-delay estimate against explicit read-completion times.
// ABOUTME: Keeps restart, error and empty-read intervals out of the continuous-read estimate.

#[path = "../../firmware/src/capture_timing.rs"]
mod capture_timing;

use capture_timing::ReadServiceTiming;

#[test]
fn subtracts_received_audio_time_from_the_read_completion_interval() {
    let mut timing = ReadServiceTiming::new(48_000);
    assert_eq!(timing.complete(1_000, 240), 0);
    assert_eq!(timing.complete(6_000, 240), 0);
    assert_eq!(timing.complete(11_400, 240), 400);
    assert_eq!(timing.complete(12_500, 48), 100);
    assert_eq!(timing.complete(12_600, 240), 0);
}

#[test]
fn excludes_restart_error_and_empty_read_intervals() {
    let mut timing = ReadServiceTiming::new(48_000);
    timing.complete(1_000, 240);
    timing.reset();
    assert_eq!(timing.complete(1_000_000, 240), 0);
    assert_eq!(timing.complete(2_000_000, 0), 0);
    assert_eq!(timing.complete(3_000_000, 240), 0);
    assert_eq!(timing.complete(3_005_500, 240), 500);
}

#[test]
fn clamps_long_intervals_without_wrapping_the_maximum() {
    let mut timing = ReadServiceTiming::new(48_000);
    timing.complete(1_000, 240);
    assert_eq!(timing.complete(2 * u64::from(u32::MAX), 240), u32::MAX);
}
