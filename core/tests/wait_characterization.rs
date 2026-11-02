// ABOUTME: Characterizes the original callable wait completion boundary before extraction.
// ABOUTME: Forces completion beyond expiry instead of racing a short sleep near the edge.
use esp_shazam_core::stream_wait::{read_fresh, WaitError};
use std::time::{Duration, Instant};
#[test]
fn an_attempt_finishing_late_keeps_its_success_or_read_error() {
    for expected in [Ok(7), Err("overrun"), Ok(0)] {
        let result = read_fresh(Duration::from_millis(1), Duration::from_millis(1), || {
            let started = Instant::now();
            while started.elapsed() < Duration::from_millis(2) {
                std::hint::spin_loop();
            }
            expected
        });
        match expected {
            Ok(0) => assert_eq!(result, Err(WaitError::Stalled)),
            Ok(7) => assert_eq!(result, Ok(7)),
            Err(e) => assert_eq!(result, Err(WaitError::Read(e))),
            _ => unreachable!(),
        }
    }
}
