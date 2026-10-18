// ABOUTME: Models the production listener guard and encoder commit boundary with Loom.
// ABOUTME: Keeps ordinary sockets, channels, clocks and device scheduling outside the model.
#![cfg(feature = "listener-loom")]
// These imports also carry ordinary channel/codec methods outside this model boundary.
#[allow(dead_code)]
#[path = "../../firmware/src/close_reaper.rs"]
mod close_reaper;
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

    /// An encoder laps a two-frame ring while a live listener resynchronizes under the mutex.
    /// Whatever the interleaving, the cursor move lands on a whole frame the next read copies,
    /// or the read reports a terminal reset; it never produces `InvalidOffset`.
    #[test]
    fn encoder_wrap_never_leaves_a_resyncing_listener_on_a_partial_frame() {
        check(|| {
            use esp_shazam_core::flac_ring::{FlacRing, ReadError, SourceMetadata};
            let source = SourceMetadata {
                raw_generation: 0,
                source_start: 0,
                source_end: 1,
                oldest_accepted_us: 0,
                raw_publication_us: 0,
            };
            let ring = Arc::new(loom::sync::Mutex::new(FlacRing::try_new(2, 4).unwrap()));
            let mut cursor = ring.lock().unwrap().cursor_now();
            let encoder = thread::spawn({
                let ring = ring.clone();
                move || {
                    // Three distinct-length frames cross the wrap; a reset ends the generation.
                    for frame in [b"a" as &[u8], b"bb", b"ccc"] {
                        ring.lock().unwrap().push(frame, source, || 0).unwrap();
                    }
                    ring.lock().unwrap().reset().unwrap();
                }
            });
            let mut out = [0; 4];
            for _ in 0..3 {
                let guard = ring.lock().unwrap();
                let read = match guard.read(&mut cursor, &mut out) {
                    Err(ReadError::Overwritten) => {
                        let passed = guard.catch_up(&mut cursor).unwrap();
                        assert!(passed > 0, "a lapped cursor always has a newer whole frame");
                        guard.read(&mut cursor, &mut out)
                    }
                    other => other,
                };
                match read {
                    Ok(read) => {
                        // Whole frames only: the copy is the frame's full length.
                        let lengths = [0, 1, 2, 3];
                        assert!(
                            read.receipt.is_none_or(
                                |receipt| lengths[receipt.sequence as usize] == read.bytes
                            )
                        );
                    }
                    Err(ReadError::GenerationChanged) => break,
                    Err(ReadError::Overwritten) => unreachable!("catch_up lands within retention"),
                    Err(ReadError::InvalidOffset) => panic!("cursor left mid-frame"),
                }
            }
            encoder.join().unwrap();
        });
    }

    /// One listener hands its socket to the reaper and reconnects at once; the other leaves
    /// while this thread reaps. Owners never exceed two, draining sockets never exceed two,
    /// and a hand-off at the drain limit waits for a finished drain while keeping its slot.
    #[test]
    fn admission_racing_a_drain_never_exceeds_two_active_and_two_draining() {
        check(|| {
            use crate::close_reaper::Reaper;
            let capacity = Arc::new(esp_shazam_core::stream_wait::Capacity::default());
            let reaper = Arc::new(Reaper::<u64>::new());
            let first = Slot::take(&capacity, 2).unwrap();
            let second = Slot::take(&capacity, 2).unwrap();
            fn hand_off(slot: Slot, reaper: &Reaper<u64>) {
                let permit = reaper.reserve().unwrap();
                let id = slot.owner_id();
                drop(slot);
                permit.submit(id);
            }
            let reconnecting = {
                let (reaper, capacity) = (reaper.clone(), capacity.clone());
                thread::spawn(move || {
                    hand_off(first, &reaper);
                    // Its own hand-off freed a slot, so the reconnect is always admitted; it
                    // hands off again, waiting at the drain limit until a drain finishes.
                    hand_off(Slot::take(&capacity, 2).unwrap(), &reaper);
                })
            };
            let leaving = {
                let reaper = reaper.clone();
                thread::spawn(move || hand_off(second, &reaper))
            };
            // This thread is the reaper: three hand-offs always arrive, and the reconnect's
            // may precede the second departure's.
            let mut reaped = Vec::new();
            for _ in 0..3 {
                reaped.push(reaper.next().unwrap());
                reaper.finished();
            }
            reconnecting.join().unwrap();
            leaving.join().unwrap();
            reaped.sort_unstable();
            assert_eq!(reaped, [1, 2, 3]);
            assert_eq!(capacity.held(), 0);
            assert_eq!(reaper.draining(), 0);
            assert_eq!(Arc::strong_count(&reaper), 1);
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
