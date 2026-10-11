// ABOUTME: Compares real raw and encoded histories with append-only reference logs.
// ABOUTME: Generates bounded reader, reset, reconnect and listener ownership sequences.
#![cfg(not(any(feature = "capture-loom", feature = "listener-loom")))]
use esp_shazam_core::{
    capture_buffer::CaptureBuffer,
    flac_ring::{FlacRing, InvalidFrame, ReadError as EncodedReadError},
    stream_ring::ReadError,
    stream_wait::Slot,
};
use proptest::prelude::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

// Default config retains environment overrides, shrinking and source-relative regressions.
fn config() -> ProptestConfig {
    let mut config = ProptestConfig {
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"),
        )),
        ..ProptestConfig::default()
    };
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = 512;
    }
    if std::env::var_os("PROPTEST_RNG_SEED").is_none() {
        config.rng_seed = proptest::test_runner::RngSeed::Fixed(15966833);
    }
    config
}

proptest! {
    #![proptest_config(config())]
    // Catches lost staging, wrong retention boundary, swapped samples and reset resurrection.
    #[test]
    fn raw_history_matches_log(block in 1usize..6, slots in 1usize..7,
        operations in prop::collection::vec((0u8..7, 0usize..25), 1..180)) {
        let capacity = block * slots;
        let (mut publisher, ring) = CaptureBuffer::new(capacity, block);
        let mut cursors = [ring.cursor_now().unwrap(); 2];
        let mut positions = [0usize; 2];
        let mut generations = [0usize; 2];
        let mut generation = 0;
        let mut log = Vec::<[i32; 2]>::new();
        let mut accepted_times = Vec::<u64>::new();
        let mut committed = 0;
        let mut next = 1;
        for (tick, (op, count)) in operations.into_iter().enumerate() {
            match op {
                0 | 1 => {
                    let frames: Vec<_> = (0..count).map(|_| {let n = next; next += 1; [n, -n - 17]}).collect();
                    let mut samples: Vec<_> = frames.iter().flatten().copied().collect();
                    samples.push(i32::MIN); // Half a stereo frame must never enter staging.
                    publisher.publish(&samples, tick as u64, || tick as u64 + 1000).unwrap();
                    accepted_times.extend(std::iter::repeat_n(tick as u64, count));
                    log.extend(frames);
                    committed = log.len() / block * block;
                    prop_assert_eq!(publisher.staged_frames(), log.len() - committed);
                }
                2 | 3 => {
                    let reader = (op - 2) as usize;
                    let old = cursors[reader];
                    let mut out = vec![i32::MIN; count * 2 + 1];
                    let result = ring.read(&mut cursors[reader], &mut out);
                    if let Ok(read) = &result {
                        if read.frames > 0 {
                            let receipt = read.receipt.unwrap();
                            prop_assert_eq!(receipt.generation, generation as u64);
                            prop_assert_eq!(receipt.source_start, positions[reader] as u64);
                            prop_assert_eq!(receipt.source_end, (positions[reader] + read.frames) as u64);
                            prop_assert_eq!(receipt.oldest_accepted_us, accepted_times[positions[reader] / block * block]);
                            prop_assert_eq!(receipt.committed_at_us, accepted_times[(positions[reader] / block + 1) * block - 1] + 1000);
                        } else { prop_assert_eq!(read.receipt, None); }
                    }
                    let result = result.map(|read| read.frames);
                    if generations[reader] != generation || committed.saturating_sub(positions[reader]) > capacity {
                        prop_assert_eq!(result, Err(ReadError::Overrun));
                        prop_assert_eq!(cursors[reader], old);
                    } else {
                        let end = committed.min(positions[reader] + count).min((positions[reader] / block + 1) * block);
                        let expected: Vec<_> = log[positions[reader]..end].iter().flatten().copied().collect();
                        prop_assert_eq!(result, Ok(end - positions[reader]));
                        prop_assert_eq!(&out[..expected.len()], expected.as_slice());
                        prop_assert!(out[expected.len()..].iter().all(|&v| v == i32::MIN));
                        positions[reader] = end;
                    }
                }
                4 => {
                    prop_assert_eq!(publisher.invalidate().unwrap(), log.len() - committed);
                    log.clear(); accepted_times.clear(); committed = 0; generation += 1;
                }
                _ => {
                    let reader = (op - 5) as usize;
                    cursors[reader] = ring.cursor_now().unwrap();
                    positions[reader] = committed; generations[reader] = generation;
                }
            }
            let progress = ring.progress().unwrap();
            prop_assert_eq!(progress.generation, generation as u64);
            prop_assert_eq!(progress.accepted_frames, log.len() as u64);
            prop_assert_eq!(progress.committed_frames, committed as u64);
            prop_assert_eq!(ring.capacity(), capacity);
            prop_assert!(publisher.staged_frames() < block);
        }
    }

    // Catches wrong byte offsets, overwritten partial frames and invalid pushes changing history.
    #[test]
    fn encoded_history_matches_log(capacity in 1usize..7, max_bytes in 1usize..17,
        operations in prop::collection::vec((0u8..8, 0usize..22, any::<u8>()), 1..180)) {
        let mut ring = FlacRing::try_new(capacity, max_bytes).unwrap();
        let mut cursors = [ring.cursor_now(); 2];
        let mut positions = [(0usize, 0usize, 0usize); 2];
        let mut generation = 0;
        let mut log = Vec::<Vec<u8>>::new();
        for (op, count, value) in operations {
            match op {
                0 | 1 => {
                    let frame: Vec<_> = (0..count).map(|i| value.wrapping_add(i as u8)).collect();
                    let before = ring.progress();
                    if count == 0 || count > max_bytes {
                        prop_assert_eq!(ring.push(&frame, esp_shazam_core::flac_ring::SourceMetadata { raw_generation: 0, source_start: 0, source_end: 1, oldest_accepted_us: 0, raw_publication_us: 0 }, || 0), Err(InvalidFrame));
                        prop_assert_eq!(ring.progress(), before);
                    } else { ring.push(&frame, esp_shazam_core::flac_ring::SourceMetadata { raw_generation: 0, source_start: 0, source_end: 1, oldest_accepted_us: 0, raw_publication_us: 0 }, || 0).unwrap(); log.push(frame); }
                }
                2 | 3 => {
                    let reader = (op - 2) as usize;
                    let (epoch, index, offset) = positions[reader];
                    let old = cursors[reader];
                    let mut out = vec![99; count];
                    let result = ring.read(&mut cursors[reader], &mut out).map(|read| read.bytes);
                    if epoch != generation {
                        prop_assert_eq!(result, Err(EncodedReadError::GenerationChanged));
                        prop_assert_eq!(cursors[reader], old);
                        prop_assert!(out.iter().all(|&v| v == 99));
                    } else if log.len().saturating_sub(index) > capacity {
                        prop_assert_eq!(result, Err(EncodedReadError::Overwritten));
                        prop_assert!(out.iter().all(|&v| v == 99));
                        prop_assert_eq!(cursors[reader], old);
                    } else if index == log.len() { prop_assert_eq!(result, Ok(0)); }
                    else {
                        let remaining = &log[index][offset..];
                        let n = count.min(remaining.len());
                        prop_assert_eq!(result, Ok(n));
                        prop_assert_eq!(&out[..n], &remaining[..n]);
                        prop_assert!(out[n..].iter().all(|&v| v == 99));
                        positions[reader] = if n == remaining.len() {(epoch, index + 1, 0)} else {(epoch, index, offset + n)};
                    }
                }
                4 => { ring.reset().unwrap(); generation += 1; log.clear(); }
                5 | 6 => {
                    let reader = (op - 5) as usize;
                    cursors[reader] = ring.cursor_now(); positions[reader] = (generation, log.len(), 0);
                }
                _ => {
                    let wanted = count % capacity + 1;
                    let retained = ring.cursor_retained(wanted).unwrap();
                    prop_assert_eq!(retained.is_some(), log.len() >= wanted);
                    if let Some(cursor) = retained { cursors[0] = cursor; positions[0] = (generation, log.len() - wanted, 0); }
                }
            }
            prop_assert_eq!(ring.progress(), (generation as u64, log.len() as u64));
            prop_assert_eq!(ring.capacity(), capacity);
            prop_assert_eq!(ring.max_frame_bytes(), max_bytes);
        }
    }

    // Catches a third admission, leaked guards on cancellation, and failed reconnect after release.
    #[test]
    fn listener_ownership_survives_reconnect_and_cancel(operations in prop::collection::vec((0u8..4, 0usize..5), 1..240)) {
        let taken = Arc::new(AtomicUsize::new(0));
        let mut guards: Vec<Option<Slot>> = (0..5).map(|_| None).collect();
        let mut active = [false; 5];
        for (op, index) in operations {
            match op {
                0 | 1 if !active[index] => {
                    let expected = active.iter().filter(|&&v| v).count() < 2;
                    guards[index] = Slot::take(&taken, 2);
                    prop_assert_eq!(guards[index].is_some(), expected);
                    active[index] = expected;
                }
                2 => { drop(guards[index].take()); active[index] = false; }
                3 => {
                    // Returning from setup before ownership is handed to a worker is cancellation.
                    let before = active.iter().filter(|&&v| v).count();
                    let cancelled = Slot::take(&taken, 2);
                    prop_assert_eq!(cancelled.is_some(), before < 2);
                    drop(cancelled);
                }
                _ => {}
            }
            let expected = active.iter().filter(|&&v| v).count();
            prop_assert_eq!(taken.load(Ordering::Acquire), expected);
            prop_assert!(expected <= 2);
        }
        drop(guards);
        prop_assert_eq!(taken.load(Ordering::Acquire), 0);
        let first = Slot::take(&taken, 2).unwrap();
        let second = Slot::take(&taken, 2).unwrap();
        prop_assert!(Slot::take(&taken, 2).is_none());
        drop((first, second));
        prop_assert_eq!(taken.load(Ordering::Acquire), 0);
    }
}

#[path = "../../firmware/src/stream_codec.rs"]
mod stream_codec;
#[path = "../../firmware/src/stream_runtime.rs"]
mod stream_runtime;

proptest! {
    #![proptest_config(config())]
    // Catches cancelled same-codec Play consuming admission or retaining dead listener epochs.
    #[test]
    fn runtime_cancel_reconnect_releases_epoch(operations in prop::collection::vec((any::<bool>(), 0usize..3), 1..40)) {
        use esp_shazam_core::stream_buffer::OutputFormat;
        use std::{sync::Mutex, time::Duration};
        let (runtime, worker) = stream_runtime::Runtime::new(OutputFormat::Flac, false).unwrap();
        let weak = Arc::downgrade(&worker.epoch);
        let taken = Arc::new(AtomicUsize::new(0));
        let mut listeners = Vec::new();
        for (cancel, operation) in operations {
            match operation {
                0 => {
                    if let Some(slot) = Slot::take(&taken, 2) {
                        let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
                        listeners.push((lease.unwrap(), slot));
                    }
                }
                1 => { listeners.pop(); }
                _ => {
                    let result = runtime.play_format(OutputFormat::Flac, Arc::new(Mutex::new(!cancel)), &taken, 2, Duration::ZERO);
                    if cancel { prop_assert!(result.is_err()); }
                    else if listeners.len() == 2 { prop_assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut); }
                    else { prop_assert_eq!(result.unwrap(), true); }
                }
            }
            prop_assert_eq!(taken.load(Ordering::Acquire), listeners.len());
            prop_assert_eq!(runtime.active_format(), OutputFormat::Flac);
            prop_assert!(!worker.epoch.retired());
        }
        drop(runtime); drop(worker);
        prop_assert_eq!(weak.upgrade().is_some(), !listeners.is_empty());
        drop(listeners);
        prop_assert!(weak.upgrade().is_none());
        prop_assert_eq!(taken.load(Ordering::Acquire), 0);
    }
}
