// ABOUTME: Models the production listener guard and encoder commit boundary with Loom.
// ABOUTME: Keeps ordinary sockets, channels, clocks and device scheduling outside the model.
#![cfg(feature = "listener-loom")]
// These imports also carry ordinary channel/codec methods outside this model boundary.
#[allow(dead_code)]
#[path = "../../firmware/src/stream_codec.rs"]
mod stream_codec;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_runtime.rs"]
mod stream_runtime;

mod models {
    use esp_shazam_core::stream_wait::Slot;
    use loom::sync::Arc;
    use loom::thread;

    pub(crate) fn check(f: impl Fn() + Send + Sync + 'static) {
        let mut builder = loom::model::Builder::new();
        builder.max_threads = 4;
        builder.max_branches = 1000;
        builder.preemption_bound = Some(2);
        builder.max_permutations = Some(10000);
        builder.checkpoint_interval = 1;
        builder.max_duration = None;
        builder.checkpoint_file = None;
        // Outside-model bookkeeping only; never participates in a protocol assertion.
        let executions = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = executions.clone();
        builder.check(move || {
            counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            f();
        });
        let executions = executions.load(std::sync::atomic::Ordering::Relaxed);
        eprintln!("listener Loom: {executions} schedules; threads=4 branches=1000 preemptions=2 cap=10000");
        assert!(
            // Loom 0.7.2 starts its iteration counter at one, before the first run.
            executions < 9999,
            "permutation cap reached; model exploration truncated"
        );
    }

    #[test]
    fn reserve_admit_cancel_and_drop_never_create_a_third_payload_owner() {
        check(|| {
            use esp_shazam_core::{
                network_lifecycle::Network, stream_buffer::OutputFormat, stream_wait::Capacity,
            };
            let network = Network::new();
            network.observe(network.snapshot(), Some("192.0.2.10".parse().unwrap()));
            let lease = network.lease().unwrap();
            let capacity = Arc::new(Capacity::default());
            let diagnostic = Slot::take(&capacity, 2).unwrap();
            let newest = Slot::take(&capacity, 2).unwrap();
            let peer = "192.0.2.20".parse().unwrap();
            let id = capacity
                .reserve(
                    peer,
                    network,
                    lease,
                    Arc::new(loom::sync::Mutex::new(true)),
                    OutputFormat::Flac,
                    None,
                    std::time::Instant::now(),
                )
                .unwrap();
            let release = thread::spawn(move || drop(newest));
            let listener_capacity = capacity.clone();
            let listener = thread::spawn(move || {
                let slot = Capacity::admit(
                    &listener_capacity,
                    id,
                    peer,
                    lease.generation,
                    OutputFormat::Flac,
                    None,
                );
                assert!(listener_capacity.held() <= 2);
                drop(slot);
            });
            let cancellation_capacity = capacity.clone();
            let cancellation = thread::spawn(move || {
                cancellation_capacity.clear(id);
                let slot = Slot::take(&cancellation_capacity, 2);
                assert!(cancellation_capacity.held() <= 2);
                drop(slot);
            });
            release.join().unwrap();
            listener.join().unwrap();
            cancellation.join().unwrap();
            assert!(!diagnostic.revoked());
            drop(diagnostic);
            assert_eq!(capacity.held(), 0);
        });
    }

    #[test]
    fn slot_saturation() {
        check(|| {
            let count = Arc::new(esp_shazam_core::stream_wait::Capacity::default());
            // Type witness: the actual production signature must accept Loom ownership.
            let take: fn(&Arc<esp_shazam_core::stream_wait::Capacity>, usize) -> Option<Slot> =
                Slot::take;
            let workers: Vec<_> = (0..3)
                .map(|_| {
                    let count = count.clone();
                    thread::spawn(move || {
                        if let Some(slot) = take(&count, 2) {
                            assert!(slot.taken() <= 2);
                        }
                    })
                })
                .collect();
            for worker in workers {
                worker.join().unwrap();
            }
            assert_eq!(count.held(), 0);
            let first = take(&count, 2).unwrap();
            let second = take(&count, 2).unwrap();
            assert!(take(&count, 2).is_none());
            drop((first, second));
            assert_eq!(count.held(), 0);
            assert_eq!(Arc::strong_count(&count), 1);
        });
    }

    #[test]
    fn slot_release_races_admission() {
        check(|| {
            let count = Arc::new(esp_shazam_core::stream_wait::Capacity::default());
            let first = Slot::take(&count, 2).unwrap();
            let second = Slot::take(&count, 2).unwrap();
            let release = thread::spawn(move || drop(first));
            let contender_count = count.clone();
            let contender = thread::spawn(move || Slot::take(&contender_count, 2));
            release.join().unwrap();
            let admitted = contender.join().unwrap();
            assert_eq!(count.held(), 1 + usize::from(admitted.is_some()));
            drop((second, admitted));
            assert_eq!(count.held(), 0);
            assert_eq!(Arc::strong_count(&count), 1);
        });
    }

    #[test]
    fn slot_worker_normal_and_early_return() {
        check(|| {
            let count = Arc::new(esp_shazam_core::stream_wait::Capacity::default());
            let normal = Slot::take(&count, 2).unwrap();
            let early = Slot::take(&count, 2).unwrap();
            let worker = thread::spawn(move || {
                let _owned = normal;
            });
            let cancelled = thread::spawn(move || {
                let _owned = early;
                if _owned.taken() > 0 {
                    return;
                }
                unreachable!();
            });
            worker.join().unwrap();
            cancelled.join().unwrap();
            assert_eq!(count.held(), 0);
            assert_eq!(Arc::strong_count(&count), 1);
        });
    }

    #[test]
    fn unlaunched_closure_releases_slot() {
        check(|| {
            let count = Arc::new(esp_shazam_core::stream_wait::Capacity::default());
            let slot = Slot::take(&count, 2).unwrap();
            let other = Slot::take(&count, 2).unwrap();
            let unlaunched = move || drop(slot);
            let cleanup = thread::spawn(move || drop(unlaunched));
            let contender_count = count.clone();
            let contender = thread::spawn(move || Slot::take(&contender_count, 2));
            cleanup.join().unwrap();
            let admitted = contender.join().unwrap();
            drop((other, admitted));
            assert_eq!(count.held(), 0);
            assert_eq!(Arc::strong_count(&count), 1);
        });
    }
}
