// ABOUTME: Exercises the actual bounded startup trace publication and recovery observations.
// ABOUTME: Uses real threads without firmware hardware or mocked driver behavior.
#[path = "../../firmware/src/startup_capture_trace.rs"]
mod trace;
use trace::{Bank, Observer, Record};
#[test]
fn delayed_export_preserves_first_records_and_reports_drops() {
    let bank = Bank::<2>::new();
    bank.publish(Record::new(1, 10));
    bank.publish(Record::new(2, 20));
    bank.publish(Record::new(3, 30));
    assert_eq!(bank.read(0).unwrap().words[0], 1);
    assert_eq!(bank.read(1).unwrap().words[0], 2);
    assert!(bank.read(2).is_none());
    assert_eq!(bank.dropped(), 1);
}
#[test]
fn real_threads_observe_complete_immutable_payloads() {
    let bank = std::sync::Arc::new(Bank::<512>::new());
    let writer = bank.clone();
    let task = std::thread::spawn(move || {
        for n in 0..512 {
            let mut r = Record::new(n, u64::MAX - u64::from(n));
            r.words[3..].fill(n);
            writer.publish(r);
        }
    });
    for n in 0..512 {
        loop {
            if let Some(r) = bank.read(n) {
                assert_eq!(r.words[0], n as u32);
                assert!(r.words[3..].iter().all(|v| *v == n as u32));
                assert_eq!(r.time(), u64::MAX - n as u64);
                break;
            }
            std::thread::yield_now();
        }
    }
    task.join().unwrap();
    assert_eq!(bank.dropped(), 0);
}
#[test]
fn post_flush_baseline_and_two_recoveries_preserve_read_and_error_results() {
    let mut observer = Observer::new();
    observer.baseline(15);
    assert!(observer.fault(15, 100, 200, 0).is_none());
    observer.read(110, 120, 240, 0);
    let fault = observer.fault(16, 130, 201, 23900).unwrap();
    assert_eq!(&fault.words[3..11], &[15, 16, 201, 23900, 110, 0, 120, 0]);
    assert_eq!(fault.words[11], 240);
    observer.restarted();
    observer.read(140, 150, 0, -1);
    let fault = observer.fault(17, 160, 202, 24000).unwrap();
    assert_eq!(fault.words[12] as i32, -1);
    observer.restarted();
    assert!(observer.complete());
    assert!(observer.fault(18, 170, 203, 24000).is_none());
}
