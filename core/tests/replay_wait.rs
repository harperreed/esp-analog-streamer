// ABOUTME: Pins completion and admission deadline edges in the shared wait decisions.
// ABOUTME: Uses literal monotonic times without sleeping near an equality boundary.
use esp_shazam_core::stream_wait::{FreshWait, WaitStep};
use std::time::Duration;
#[test]
fn nonempty_and_error_completion_win_over_expiry_but_empty_does_not() {
    for ms in [9, 10, 11] {
        let wait = FreshWait::new(
            Duration::ZERO,
            Duration::from_millis(10),
            Duration::from_millis(1),
        );
        assert_eq!(
            wait.finish(Ok::<_, ()>(1), &mut || Duration::from_millis(ms)),
            WaitStep::Ready(1)
        );
        assert_eq!(
            wait.finish(Err("overrun"), &mut || Duration::from_millis(ms)),
            WaitStep::Failed("overrun")
        );
        let result = wait.finish(Ok::<_, ()>(0), &mut || Duration::from_millis(ms));
        if ms < 10 {
            assert_eq!(result, WaitStep::SleepUntil(Duration::from_millis(10)));
        } else {
            assert_eq!(result, WaitStep::Stalled);
        }
    }
}

#[test]
fn retry_inner_equality_sleeps_then_restarts_only_inner_budget() {
    use esp_shazam_core::{stream_ring::ReadError, stream_wait::RawWait};
    let mut wait = RawWait::new(
        Duration::ZERO,
        Duration::from_millis(10),
        Duration::from_millis(2),
    );
    wait.before_attempt(&mut || Duration::ZERO);
    let mut times = [Duration::from_micros(999), Duration::from_millis(9)].into_iter();
    assert_eq!(
        wait.finish(Err(ReadError::Retry), &mut || times.next().unwrap()),
        WaitStep::RetryNow
    );
    let mut times = [Duration::from_millis(1), Duration::from_millis(1)].into_iter();
    assert_eq!(
        wait.finish(Err(ReadError::Retry), &mut || times.next().unwrap()),
        WaitStep::SleepUntil(Duration::from_millis(3))
    );
    wait.before_attempt(&mut || Duration::from_millis(3));
    let mut times = [
        Duration::from_micros(3999),
        Duration::from_millis(10),
        Duration::from_millis(10),
    ]
    .into_iter();
    assert_eq!(
        wait.finish(Err(ReadError::Retry), &mut || times.next().unwrap()),
        WaitStep::Stalled
    );
}

#[test]
fn buffered_reset_progress_does_not_renew_absolute_admission_deadline() {
    use esp_shazam_core::stream_wait::{BufferedWait, SnapshotWait};
    let mut wait = BufferedWait::new(
        Duration::ZERO,
        Duration::from_millis(10),
        Duration::from_millis(3),
        Duration::from_millis(1),
    );
    wait.observe_progress((0, 9), &mut || {
        panic!("first snapshot establishes no progress")
    });
    assert!(wait.before_attempt(Duration::from_millis(2)));
    assert!(!wait.before_attempt(Duration::from_millis(3)));
    wait.observe_progress((1, 0), &mut || {
        panic!("empty generation reset is not progress")
    });
    wait.observe_progress((1, 1), &mut || Duration::from_millis(2));
    assert!(wait.before_attempt(Duration::from_millis(4)));
    wait.observe_progress((2, 1), &mut || Duration::from_millis(9));
    assert!(wait.before_attempt(Duration::from_millis(9)));
    assert!(!wait.before_attempt(Duration::from_millis(10)));
    // A retained result admitted before expiry has no completion-time veto.
    assert_eq!(wait.retained(Ok::<_, ()>(Some(23))), Ok(Some(23)));
    let snapshot = SnapshotWait::new(Duration::from_millis(10), Duration::from_millis(1));
    assert!(snapshot.before_attempt(Duration::from_millis(9)));
    assert!(!snapshot.before_attempt(Duration::from_millis(10)));
    assert!(!snapshot.before_attempt(Duration::from_millis(11)));
}
