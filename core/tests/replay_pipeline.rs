// ABOUTME: Drives the real firmware orchestration with literal virtual time and actual codec data.
// ABOUTME: Pins partial input retention, generation reset and listener completion boundaries.
#![cfg(not(feature = "listener-loom"))]
#[allow(dead_code)]
#[path = "../../firmware/src/stream_codec.rs"]
mod stream_codec;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_pipeline.rs"]
mod stream_pipeline;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_runtime.rs"]
mod stream_runtime;
use esp_shazam_core::{
    capture_buffer::CaptureBuffer, stream_buffer::OutputFormat, stream_http::ListenerPolicy,
};
use std::time::Duration;
use stream_pipeline::{EncoderPump, EncoderStep};
use stream_runtime::Runtime;
#[test]
fn absent_capture_retains_partial_input_and_generation_reset_discards_it() {
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(3840, 240);
    let mut pump = EncoderPump::new();
    let mut time = Duration::ZERO;
    assert!(matches!(
        pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {}),
        EncoderStep::Progress
    ));
    publisher.publish(&vec![17; 480], 0, || 0).unwrap();
    pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {});
    assert_eq!(worker.encoder.frames_wanted(), 3856);
    pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {});
    time = Duration::from_secs(2);
    pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {});
    assert_eq!(worker.encoder.frames_wanted(), 3856);
    publisher.invalidate().unwrap();
    pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {});
    assert_eq!(worker.encoder.frames_wanted(), 4096);
    assert_eq!(worker.epoch.ring.lock().unwrap().progress(), (1, 0));
}

#[test]
fn close_failure_after_release_retains_only_the_socket() {
    use esp_shazam_core::stream_wait::Slot;
    use std::sync::Arc;
    use stream_pipeline::ConnectionResources;
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit_line("GET /stream.flac HTTP/1.1");
    let slots = Arc::new(esp_shazam_core::stream_wait::Capacity::default());
    let resources = ConnectionResources::new((), Slot::take(&slots, 1).unwrap(), lease);
    let draining = resources.release(|| 7);
    assert_eq!(slots.held(), 0, "the slot is released at hand-off");
    assert_eq!(draining.released_us, 7);
    let request = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    );
    assert!(
        worker.service(&runtime),
        "the released lease no longer blocks an idle switch"
    );
    let stream_runtime::SwitchStart::Pending(pending) = request else {
        panic!("pending")
    };
    assert!(matches!(
        pending.poll(),
        stream_runtime::SwitchPoll::Finished(Ok(true))
    ));
    let retained = draining
        .close(|_| Err::<(), _>("uncertain close"))
        .err()
        .unwrap();
    // The failure owns whatever the close left behind until its owner decides.
    drop(retained);
    assert_eq!(slots.held(), 0);
}

#[test]
fn pending_header_cannot_be_preempted_and_payload_accounting_requires_complete_write() {
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let old = worker.epoch.clone();
    let mut time = Duration::ZERO;
    let mut listener =
        CompressedListener::new(&old, None, false, ListenerPolicy::Live, &mut || time);
    let mut bytes = vec![0; OutputFormat::Flac.max_frame_bytes()];
    assert!(matches!(
        listener.step(&old, &mut bytes, &mut || time, &mut || panic!(
            "live must not probe"
        )),
        ListenerStep::Progress
    ));
    let ListenerStep::Write {
        deadline,
        header: true,
    } = listener.step(&old, &mut bytes, &mut || time, &mut || true)
    else {
        panic!("header")
    };
    assert!(listener.write_bytes(&bytes).starts_with(b"HTTP/1.1 200"));
    let request = runtime.begin_switch(
        OutputFormat::Mp3,
        true,
        std::sync::Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    );
    assert!(worker.service(&runtime));
    time = Duration::from_secs(100);
    assert!(
        matches!(listener.step(&old, &mut bytes, &mut || time, &mut || panic!("pending write must not probe")), ListenerStep::Write { deadline: d, header: true } if d == deadline)
    );
    assert!(matches!(
        listener.write_finished(Ok(()), &mut || Duration::ZERO),
        ListenerStep::Progress
    ));
    assert!(matches!(
        listener.step(&old, &mut bytes, &mut || time, &mut || true),
        ListenerStep::End(End::Retired)
    ));
    drop(request);
    assert_eq!(listener.frames_sent, 0);
}

#[test]
fn retirement_during_fresh_wait_is_retired_and_empty_expiry_is_stalled() {
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    for elapsed in [
        Duration::from_millis(500),
        Duration::from_secs(2),
        Duration::from_secs(3),
    ] {
        for retire in [false, true] {
            if !retire && elapsed < Duration::from_secs(2) {
                continue;
            }
            let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
            let epoch = worker.epoch.clone();
            let mut listener =
                CompressedListener::new(&epoch, None, false, ListenerPolicy::Live, &mut || {
                    Duration::ZERO
                });
            let mut bytes = vec![0; OutputFormat::Flac.max_frame_bytes()];
            listener.step(&epoch, &mut bytes, &mut || Duration::ZERO, &mut || true);
            listener.step(&epoch, &mut bytes, &mut || Duration::ZERO, &mut || true);
            listener.write_finished(Ok(()), &mut || Duration::ZERO);
            assert!(matches!(
                listener.step(&epoch, &mut bytes, &mut || Duration::ZERO, &mut || true),
                ListenerStep::SleepUntil(at) if at == Duration::from_millis(10)
            ));
            if retire {
                let _pending = runtime.begin_switch(
                    OutputFormat::Mp3,
                    true,
                    std::sync::Arc::new(std::sync::Mutex::new(true)),
                    Duration::from_secs(1),
                );
                assert!(worker.service(&runtime));
            }
            let end = listener.step(&epoch, &mut bytes, &mut || elapsed, &mut || true);
            assert_eq!(listener.read_wait, elapsed);
            assert!(matches!(
                (retire, end),
                (true, ListenerStep::End(End::Retired))
                    | (false, ListenerStep::End(End::CaptureStalled))
            ));
        }
    }
}

#[test]
fn close_callback_runs_once_after_slot_and_lease_release() {
    use esp_shazam_core::stream_wait::Slot;
    use std::sync::{Arc, Mutex};
    use stream_pipeline::ConnectionResources;
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit_line("GET /stream.flac HTTP/1.1");
    let slots = Arc::new(esp_shazam_core::stream_wait::Capacity::default());
    let resources = ConnectionResources::new((), Slot::take(&slots, 1).unwrap(), lease);
    let mut calls = 0;
    assert_eq!(
        resources
            .release(|| 0)
            .close(|_| {
                calls += 1;
                assert_eq!(slots.held(), 0);
                let request = runtime.begin_switch(
                    OutputFormat::Mp3,
                    false,
                    Arc::new(Mutex::new(true)),
                    Duration::from_secs(1),
                );
                assert!(worker.service(&runtime));
                drop(request);
                Ok::<_, ()>(13)
            })
            .ok(),
        Some(13)
    );
    assert_eq!(calls, 1);
    assert_eq!(slots.held(), 0);
}

#[cfg(feature = "capture-fixture")]
#[test]
fn reset_inside_actual_atomic_copy_never_pushes_rejected_scratch() {
    use stream_pipeline::EncoderObservation;
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(3840, 240);
    let mut pump = EncoderPump::new();
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |_| {},
    );
    publisher.publish(&vec![41; 480], 0, || 0).unwrap();
    let mut accepted = Vec::new();
    let mut observe = |observation| {
        if let EncoderObservation::AcceptedInput {
            generation,
            start,
            frames,
        } = observation
        {
            accepted.push((generation, start, frames));
        }
    };
    pump.step_with_copy_hook(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut observe,
        || {
            publisher.invalidate().unwrap();
        },
    );
    assert!(accepted.is_empty());
    assert_eq!(worker.encoder.frames_wanted(), 4096);
    assert_eq!(worker.epoch.ring.lock().unwrap().progress(), (1, 0));
}

#[derive(Clone, Copy, PartialEq)]
enum WriteOutcome {
    Complete,
    TimedOut,
    /// The deadline ran out and the frame aged past the media bound while it was on the wire.
    TimedOutStale,
}

#[test]
fn payload_counters_advance_only_after_a_whole_successful_write() {
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    for (policy, outcome) in [
        (ListenerPolicy::Exact, WriteOutcome::Complete),
        (ListenerPolicy::Exact, WriteOutcome::TimedOut),
        (ListenerPolicy::Exact, WriteOutcome::TimedOutStale),
        (ListenerPolicy::Live, WriteOutcome::Complete),
        (ListenerPolicy::Live, WriteOutcome::TimedOut),
        (ListenerPolicy::Live, WriteOutcome::TimedOutStale),
    ] {
        let successful = outcome == WriteOutcome::Complete;
        let (_, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let epoch = worker.epoch.clone();
        let mut listener =
            CompressedListener::new(&epoch, None, false, policy, &mut || Duration::ZERO);
        let mut scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
        listener.step(&epoch, &mut scratch, &mut || Duration::ZERO, &mut || true);
        assert!(matches!(
            listener.step(&epoch, &mut scratch, &mut || Duration::ZERO, &mut || true),
            ListenerStep::Write { header: true, .. }
        ));
        listener.write_finished(Ok(()), &mut || Duration::ZERO);
        worker.encoder.push(&vec![0; 8192]);
        worker.encoder.encode_block(0, &mut worker.bytes).unwrap();
        let payload_len = worker.bytes.len() as u64;
        epoch
            .ring
            .lock()
            .unwrap()
            .push(
                &worker.bytes,
                esp_shazam_core::flac_ring::SourceMetadata {
                    raw_generation: 0,
                    source_start: 0,
                    source_end: 1,
                    oldest_accepted_us: 0,
                    raw_publication_us: 0,
                },
                || 0,
            )
            .unwrap();
        assert!(matches!(
            listener.step(&epoch, &mut scratch, &mut || Duration::ZERO, &mut || true),
            ListenerStep::Write { header: false, .. }
        ));
        assert_eq!(listener.pending_oldest_accepted_us(), Some(0));
        assert_eq!(listener.frames_sent, 0);
        assert_eq!(listener.first_sent, None);
        assert_eq!(listener.last_sent, None);
        assert_eq!(listener.bytes_sent, 0);
        assert_eq!(listener.write_bytes(&scratch), worker.bytes);
        // Reobserving a pending Write neither sends nor accounts any bytes.
        listener.step(
            &epoch,
            &mut scratch,
            &mut || Duration::from_secs(100),
            &mut || panic!("pending write must not probe"),
        );
        assert_eq!((listener.frames_sent, listener.bytes_sent), (0, 0));
        if successful {
            assert!(matches!(
                listener.write_finished(Ok(()), &mut || Duration::ZERO),
                ListenerStep::Progress
            ));
            assert_eq!(
                (listener.frames_sent, listener.bytes_sent),
                (1, payload_len)
            );
        } else {
            let finished = if outcome == WriteOutcome::TimedOutStale {
                Duration::from_secs(5)
            } else {
                Duration::ZERO
            };
            let end = listener.write_finished(
                Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "partial payload expired",
                )),
                &mut || finished,
            );
            // A partial frame on the wire is a dead socket in both policies; only an exact reader
            // still reports the media bound it was also past.
            assert!(
                matches!(
                    (policy, outcome, &end),
                    (
                        ListenerPolicy::Exact,
                        WriteOutcome::TimedOutStale,
                        ListenerStep::End(End::MediaAge)
                    ) | (_, _, ListenerStep::End(End::StoppedReading))
                ),
                "{policy:?} {:?}",
                match end {
                    ListenerStep::End(end) => format!("{end:?}"),
                    _ => "continued".into(),
                }
            );
            assert_eq!((listener.frames_sent, listener.bytes_sent), (0, 0));
            assert_eq!(listener.first_sent, None);
            assert_eq!(listener.last_sent, None);
        }
        assert_eq!((listener.skips, listener.frames_skipped), (0, 0));
        if successful {
            assert!(listener.first_sent.is_some());
            assert!(listener.last_sent.is_some());
        }
    }
}

#[test]
fn release_reports_slots_held_at_the_release_before_the_close() {
    use esp_shazam_core::stream_wait::Slot;
    use std::sync::Arc;
    let slots = Arc::new(esp_shazam_core::stream_wait::Capacity::default());
    let own = Slot::take(&slots, 2).unwrap();
    let other = Slot::take(&slots, 2).unwrap();
    let resources = stream_pipeline::ConnectionResources::new((), own, None);
    let draining = resources.release(|| 0);
    assert_eq!(draining.held, 2, "held counts this socket at its release");
    assert_eq!(slots.held(), 1);
    // Another release completes while this socket is still draining.
    drop(other);
    assert_eq!(slots.held(), 0);
    assert_eq!(draining.close(|_| Ok::<_, ()>(37)).ok(), Some(37));
}

#[test]
fn cursor_acquisition_reports_the_actual_join_position_before_any_input() {
    use stream_pipeline::EncoderObservation;
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(3840, 240);
    publisher.publish(&vec![31; 480], 0, || 0).unwrap();
    let mut pump = EncoderPump::new();
    let mut acquired = Vec::new();
    let mut accepted = 0;
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |observation| match observation {
            EncoderObservation::CursorAcquired { generation, start } => {
                acquired.push((start, generation))
            }
            EncoderObservation::AcceptedInput { .. } => accepted += 1,
            _ => {}
        },
    );
    assert_eq!(acquired, [(240, 0)]);
    assert_eq!(acquired[0], raw.cursor_now().unwrap().position());
    assert_eq!(accepted, 0);
    assert_eq!(worker.encoder.frames_wanted(), 4096);
}

#[cfg(feature = "encoder-probe")]
#[test]
fn probe_push_clocks_only_when_enabled_and_storage_is_real() {
    use stream_pipeline::EncoderObservation;
    for enabled in [false, true] {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
        let (mut publisher, raw) = CaptureBuffer::new(3840, 240);
        let mut pump = EncoderPump::new();
        pump.set_probe(enabled);
        let (address, len, capacity) = pump.storage_receipt();
        assert_ne!(address, 0);
        assert_eq!(len, 1920);
        assert!(capacity >= len);
        let receipt = worker.encoder.mp3_storage_receipt().unwrap();
        assert_eq!(receipt.1, 4608);
        assert!(worker.bytes.capacity() >= 960);
        pump.step(
            &raw,
            &mut worker,
            &runtime,
            &mut || Duration::ZERO,
            &mut |_| {},
        );
        publisher.publish(&vec![12345; 480], 0, || 0).unwrap();
        let mut clocks = 0;
        let mut pushes = Vec::new();
        pump.step(
            &raw,
            &mut worker,
            &runtime,
            &mut || {
                clocks += 1;
                Duration::from_micros(clocks)
            },
            &mut |event| {
                if let EncoderObservation::PcmPush(elapsed) = event {
                    pushes.push(elapsed)
                }
            },
        );
        assert_eq!(worker.encoder.frames_wanted(), 912);
        assert_eq!(clocks, if enabled { 7 } else { 5 });
        assert_eq!(
            pushes,
            if enabled {
                vec![Duration::from_micros(1)]
            } else {
                vec![]
            }
        );
        assert_eq!(worker.encoder.mp3_storage_receipt(), Some(receipt));
    }
}

#[test]
fn wrapper_reports_and_delays_at_the_actual_eighth_publication_boundary() {
    let mut count = 0;
    let mut events = Vec::new();
    for _ in 0..9 {
        stream_pipeline::finish_encoder_turn(
            true,
            OutputFormat::Mp3,
            &mut count,
            &mut events,
            |events| events.push("report"),
            |events| events.push("delay"),
        );
    }
    #[cfg(not(feature = "encoder-probe"))]
    assert_eq!(
        events,
        [
            "report", "report", "report", "report", "report", "report", "report", "report",
            "delay", "report"
        ]
    );
    #[cfg(feature = "encoder-probe")]
    assert_eq!(
        events,
        [
            "report", "report", "report", "report", "report", "report", "report", "delay",
            "report", "report"
        ]
    );
    assert_eq!(count, 1);
}

#[cfg(feature = "encoder-probe")]
#[test]
fn empty_wait_crosses_report_threshold_without_splitting_raw_wait_or_phase() {
    use stream_pipeline::EncoderObservation;
    let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let (_, raw) = CaptureBuffer::new(3840, 240);
    let mut pump = EncoderPump::new();
    pump.set_probe(true);
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::from_secs(9),
        &mut |_| {},
    );
    let mut completed = Vec::new();
    for millis in [9000, 9500, 10500] {
        assert!(matches!(
            pump.step(
                &raw,
                &mut worker,
                &runtime,
                &mut || Duration::from_millis(millis),
                &mut |event| match event {
                    EncoderObservation::RawWait(wait) => completed.push(wait),
                    EncoderObservation::Publication { .. }
                    | EncoderObservation::Reset
                    | EncoderObservation::Stalled
                    | EncoderObservation::CursorWaitExpired =>
                        panic!("report boundary before wait completion"),
                    _ => {}
                }
            ),
            EncoderStep::SleepUntil(_)
        ));
        assert!(completed.is_empty());
    }
    let mut stalled = false;
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::from_secs(11),
        &mut |event| match event {
            EncoderObservation::RawWait(wait) => completed.push(wait),
            EncoderObservation::Stalled => {
                assert_eq!(completed, [Duration::from_secs(2)]);
                stalled = true;
            }
            _ => {}
        },
    );
    assert!(stalled);
}

#[test]
fn encoded_failures_keep_identity_and_wait_accounting_at_freshness_boundaries() {
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    for elapsed in [
        Duration::from_millis(500),
        Duration::from_secs(2),
        Duration::from_secs(3),
    ] {
        for reset in [false, true] {
            for policy in [ListenerPolicy::Exact, ListenerPolicy::Live] {
                let (_runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
                let epoch = worker.epoch.clone();
                let mut listener =
                    CompressedListener::new(&epoch, None, false, policy, &mut || Duration::ZERO);
                let mut bytes = vec![0; OutputFormat::Flac.max_frame_bytes()];
                listener.step(&epoch, &mut bytes, &mut || Duration::ZERO, &mut || true);
                listener.step(&epoch, &mut bytes, &mut || Duration::ZERO, &mut || true);
                listener.write_finished(Ok(()), &mut || Duration::ZERO);
                assert!(matches!(
                    listener.step(&epoch, &mut bytes, &mut || Duration::ZERO, &mut || true),
                    ListenerStep::SleepUntil(_)
                ));
                worker.encoder.push(&vec![0; 8192]);
                worker.encoder.encode_block(0, &mut worker.bytes).unwrap();
                let capacity = {
                    let mut ring = epoch.ring.lock().unwrap();
                    if reset {
                        ring.reset().unwrap();
                    } else {
                        for _ in 0..=ring.capacity() {
                            ring.push(
                                &worker.bytes,
                                esp_shazam_core::flac_ring::SourceMetadata {
                                    raw_generation: 0,
                                    source_start: 0,
                                    source_end: 1,
                                    oldest_accepted_us: 0,
                                    raw_publication_us: 0,
                                },
                                || 0,
                            )
                            .unwrap();
                        }
                    }
                    ring.capacity() as u64
                };
                let mut clock_calls = 0;
                let step = listener.step(
                    &epoch,
                    &mut bytes,
                    &mut || {
                        clock_calls += 1;
                        elapsed
                    },
                    &mut || true,
                );
                assert_eq!(listener.read_wait, elapsed);
                match (reset, policy) {
                    (true, _) => {
                        assert!(matches!(step, ListenerStep::End(End::EncodedReset)));
                        assert_eq!(clock_calls, 1, "failed completion must not sample expiry");
                        assert_eq!(listener.skips, 0);
                    }
                    (false, ListenerPolicy::Exact) => {
                        assert!(matches!(step, ListenerStep::End(End::FellBehind)));
                        assert_eq!(clock_calls, 1, "failed completion must not sample expiry");
                        assert_eq!(listener.skips, 0);
                    }
                    (false, ListenerPolicy::Live) => {
                        // Lapped by one: skip to the newest whole frame under the same lock and
                        // send it; the lag is recorded before the move.
                        assert!(matches!(step, ListenerStep::Write { header: false, .. }));
                        assert_eq!(
                            (
                                listener.skips,
                                listener.frames_skipped,
                                listener.last_skip_us
                            ),
                            (1, capacity, elapsed.as_micros() as u64)
                        );
                        assert_eq!(listener.queue_frames_max, capacity);
                        assert!(matches!(
                            listener.write_finished(Ok(()), &mut || elapsed),
                            ListenerStep::Progress
                        ));
                        assert_eq!(listener.frames_sent, 1);
                        assert_eq!(listener.last_sent.unwrap().sequence, capacity + 1);
                    }
                }
            }
        }
    }
}

#[test]
fn foreign_cursor_error_maps_to_invalid_cursor_without_poisoning_ring() {
    use esp_shazam_core::flac_ring::{FlacRing, ReadError};
    use stream_pipeline::End;
    let mut source = FlacRing::try_new(2, 8).unwrap();
    let mut cursor = source.cursor_now();
    source
        .push(
            b"abcdef",
            esp_shazam_core::flac_ring::SourceMetadata {
                raw_generation: 0,
                source_start: 0,
                source_end: 1,
                oldest_accepted_us: 0,
                raw_publication_us: 0,
            },
            || 0,
        )
        .unwrap();
    source
        .read(&mut cursor, &mut [0; 4])
        .map(|read| read.bytes)
        .unwrap();
    let destination = std::sync::Mutex::new(FlacRing::try_new(2, 8).unwrap());
    let mut ring = destination.lock().unwrap();
    ring.push(
        b"ab",
        esp_shazam_core::flac_ring::SourceMetadata {
            raw_generation: 0,
            source_start: 0,
            source_end: 1,
            oldest_accepted_us: 0,
            raw_publication_us: 0,
        },
        || 0,
    )
    .unwrap();
    let before = cursor;
    let mut sentinel = [99; 8];
    let error = ring
        .read(&mut cursor, &mut sentinel)
        .map(|read| read.bytes)
        .unwrap_err();
    assert_eq!(error, ReadError::InvalidOffset);
    assert!(matches!(
        End::from_encoded_read_error(error),
        End::InvalidCursor
    ));
    assert_eq!(cursor, before);
    assert_eq!(sentinel, [99; 8]);
    drop(ring);
    assert!(!destination.is_poisoned());
}

#[test]
fn network_retirement_precedes_buffered_admission_and_http_header() {
    use esp_shazam_core::{
        network_lifecycle::Network,
        stream_buffer::BufferPreset,
        stream_wait::{Capacity, Slot},
    };
    use std::sync::Arc;
    use stream_pipeline::{CompressedListener, DepartureGuard, End, ListenerStep};
    let network = Network::new();
    network.observe(network.snapshot(), Some("192.168.23.99".parse().unwrap()));
    let (_runtime, worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let capacity = Arc::new(Capacity::default());
    let slot = Slot::take(&capacity, 2).unwrap();
    for preset in [None, Some(BufferPreset::Medium)] {
        network.observe(network.snapshot(), Some("192.168.23.99".parse().unwrap()));
        let lease = network.lease().unwrap();
        let guard = DepartureGuard {
            slot: &slot,
            network: &network,
            lease,
        };
        let mut actor = CompressedListener::new(
            &worker.epoch,
            preset,
            false,
            ListenerPolicy::Live,
            &mut || Duration::ZERO,
        );
        if preset.is_none() {
            assert!(matches!(
                actor.step_guarded(
                    &worker.epoch,
                    &mut [],
                    &mut || Duration::ZERO,
                    &mut || true,
                    &guard
                ),
                ListenerStep::Progress
            ));
        }
        network.invalidate();
        assert!(matches!(
            actor.step_guarded(
                &worker.epoch,
                &mut [],
                &mut || Duration::ZERO,
                &mut || true,
                &guard
            ),
            ListenerStep::End(End::NetworkChanged)
        ));
    }
}

#[test]
fn encoded_metadata_accumulates_oldest_source_and_late_complete_counts_bytes() {
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    for policy in [ListenerPolicy::Exact, ListenerPolicy::Live] {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
        let (mut publisher, raw) = CaptureBuffer::new(4800, 240);
        let mut pump = EncoderPump::new();
        let mut time = Duration::from_micros(100);
        pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {});
        // One retained frame: two blocks published behind the listener's back lap it.
        *worker.epoch.ring.lock().unwrap() =
            esp_shazam_core::flac_ring::FlacRing::try_new(1, OutputFormat::Mp3.max_frame_bytes())
                .unwrap();
        let mut listener =
            CompressedListener::new(&worker.epoch, None, false, policy, &mut || time);
        let mut scratch = vec![0; OutputFormat::Mp3.max_frame_bytes()];
        listener.step(&worker.epoch, &mut scratch, &mut || time, &mut || true);
        listener.step(&worker.epoch, &mut scratch, &mut || time, &mut || true);
        listener.write_finished(Ok(()), &mut || time);
        let mut cursor = worker.epoch.ring.lock().unwrap().cursor_now();
        for block in 0..5 {
            publisher
                .publish(&vec![17; 480], 100 + block, || 200 + block)
                .unwrap();
            time = Duration::from_micros(300 + block);
            pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {});
        }
        let read = worker
            .epoch
            .ring
            .lock()
            .unwrap()
            .read(&mut cursor, &mut scratch)
            .unwrap();
        let metadata = read.receipt.unwrap().metadata;
        assert_eq!(metadata.source.oldest_accepted_us, 100);
        assert_eq!(
            (metadata.source.source_start, metadata.source.source_end),
            (0, 1152)
        );
        assert_eq!(metadata.source.raw_publication_us, 204);
        assert_eq!(metadata.encoded_production_us, 304);
        assert!(matches!(
            listener.step(&worker.epoch, &mut scratch, &mut || time, &mut || true),
            ListenerStep::Write { header: false, .. }
        ));
        let len = listener.write_bytes(&scratch).len();
        time = Duration::from_micros(4_000_100);
        let finished = listener.write_finished(Ok(()), &mut || time);
        assert_eq!(listener.bytes_sent, len as u64);
        assert_eq!(listener.frames_sent, 1);
        match policy {
            ListenerPolicy::Exact => {
                assert!(matches!(finished, ListenerStep::End(End::MediaAge)));
                continue;
            }
            ListenerPolicy::Live => assert!(matches!(finished, ListenerStep::Progress)),
        }
        // Two newer blocks arrive before the next read: the late frame costs one skip over the
        // block between, and the newest block goes out with the listener's numbering intact.
        for block in 5..17 {
            let at = 4_000_200 + block;
            publisher.publish(&vec![17; 480], at, || at).unwrap();
            time = Duration::from_micros(4_000_300 + block);
            pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {});
        }
        assert_eq!(worker.epoch.ring.lock().unwrap().progress(), (0, 3));
        assert!(matches!(
            listener.step(&worker.epoch, &mut scratch, &mut || time, &mut || true),
            ListenerStep::Write { header: false, .. }
        ));
        assert_eq!(
            (
                listener.skips,
                listener.frames_skipped,
                listener.last_skip_us
            ),
            (1, 1, time.as_micros() as u64)
        );
        // The owed skip records the lag it found, like a skip taken on a lapped read.
        assert_eq!(listener.queue_frames_max, 1);
        assert!(matches!(
            listener.write_finished(Ok(()), &mut || time),
            ListenerStep::Progress
        ));
        assert_eq!(listener.frames_sent, 2);
        assert_eq!(listener.last_sent.unwrap().sequence, 3);
    }
}

#[test]
fn supervisor_requests_encoder_owned_reset_and_gate_survives_ring_reset() {
    use stream_runtime::health::{State, Supervisor};
    let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(12000, 240);
    let mut supervisor = Supervisor::new();
    supervisor.observe(&runtime, &raw, 0);
    publisher.publish(&vec![0; 2400], 1, || 1).unwrap();
    supervisor.observe(&runtime, &raw, 1);
    publisher.publish(&vec![0; 480], 2, || 2).unwrap();
    supervisor.observe(&runtime, &raw, 1_999_999);
    publisher.publish(&vec![0; 480], 3, || 3).unwrap();
    supervisor.observe(&runtime, &raw, 2_000_001);
    assert_eq!(runtime.health.state(), State::ResetRequested);
    assert!(runtime.admit_line("GET /stream.mp3 HTTP/1.1").1.is_none());
    worker.epoch.ring.lock().unwrap().reset().unwrap();
    supervisor.observe(&runtime, &raw, 4_000_001);
    assert_eq!(runtime.health.state(), State::Terminal);
    let mut pump = EncoderPump::new();
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::from_secs(5),
        &mut |_| {},
    );
    assert_eq!(runtime.health.state(), State::Terminal);
}

#[test]
fn terminal_transition_is_reported_once_and_same_format_operator_replaces_it() {
    use std::sync::{Arc, Mutex};
    use stream_runtime::{
        health::{State, Supervisor},
        SwitchPoll, SwitchStart,
    };
    let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(12000, 240);
    let mut supervisor = Supervisor::new();
    supervisor.observe(&runtime, &raw, 0);
    publisher.publish(&vec![0; 2400], 1, || 1).unwrap();
    supervisor.observe(&runtime, &raw, 1);
    publisher.publish(&vec![0; 480], 2, || 2).unwrap();
    supervisor.observe(&runtime, &raw, 1_999_999);
    publisher.publish(&vec![0; 480], 3, || 3).unwrap();
    supervisor.observe(&runtime, &raw, 2_000_001);
    assert!(supervisor.observe(&runtime, &raw, 4_000_001).is_some());
    assert!(supervisor.observe(&runtime, &raw, 4_000_002).is_none());
    let stale = runtime.health.token();
    let gate = runtime.health.clone();
    let (resume, resumed) = std::sync::mpsc::channel();
    let old_observer = std::thread::spawn(move || {
        resumed.recv_timeout(Duration::from_secs(2)).unwrap();
        gate.transition(stale, State::Terminal)
    });
    let pending = match runtime.begin_switch(
        OutputFormat::Mp3,
        true,
        Arc::new(Mutex::new(true)),
        Duration::from_secs(1),
    ) {
        SwitchStart::Pending(pending) => pending,
        _ => panic!("failed same-format encoder must be prepared again"),
    };
    assert!(worker.service(&runtime));
    assert!(matches!(pending.poll(), SwitchPoll::Finished(Ok(true))));
    assert_eq!(runtime.health.state(), State::Healthy);
    resume.send(()).unwrap();
    assert!(!old_observer.join().unwrap());
    assert!(!runtime.health.transition(stale, State::Terminal));
    assert!(!runtime.health.transition(stale, State::ResetRequested));
}

#[test]
fn cooperative_reset_ack_requires_actual_new_output_to_recover() {
    use stream_runtime::health::{State, Supervisor};
    let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(12000, 240);
    let mut supervisor = Supervisor::new();
    supervisor.observe(&runtime, &raw, 0);
    publisher.publish(&vec![0; 2400], 1, || 1).unwrap();
    supervisor.observe(&runtime, &raw, 1);
    publisher.publish(&vec![0; 480], 2, || 2).unwrap();
    supervisor.observe(&runtime, &raw, 1_999_999);
    publisher.publish(&vec![0; 480], 3, || 3).unwrap();
    supervisor.observe(&runtime, &raw, 2_000_001);
    let mut pump = EncoderPump::new();
    let mut time = Duration::from_micros(2_100_000);
    let mut acks = 0;
    pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |event| {
        if matches!(event, stream_pipeline::EncoderObservation::HealthAck { .. }) {
            acks += 1;
        }
    });
    assert_eq!(acks, 1);
    assert_eq!(runtime.health.state(), State::Recovering);
    supervisor.observe(&runtime, &raw, 2_100_001);
    assert_eq!(runtime.health.state(), State::Recovering);
    assert!(runtime.health.progress().is_none());
    pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {});
    for _ in 0..5 {
        time += Duration::from_micros(5000);
        let us = time.as_micros() as u64;
        publisher.publish(&vec![0; 480], us, || us).unwrap();
        pump.step(&raw, &mut worker, &runtime, &mut || time, &mut |_| {});
    }
    let output = runtime.health.progress().unwrap();
    assert_eq!(output.receipt.generation, 1);
    assert_eq!(output.receipt.sequence, 1);
    assert!(output.receipt.metadata.source.source_start >= 1680);
    let transition = supervisor.observe(&runtime, &raw, 2_200_000).unwrap();
    assert_eq!(transition.state, State::Healthy);
    assert_eq!(transition.output.unwrap().receipt, output.receipt);
    assert_eq!(runtime.health.state(), State::Healthy);
}

#[test]
fn accepted_input_remains_distinct_while_raw_publication_is_busy() {
    use std::sync::mpsc;
    use stream_runtime::health::{State, Supervisor};
    let (runtime, _worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(4800, 240);
    let mut supervisor = Supervisor::new();
    supervisor.observe(&runtime, &raw, 0);
    let (ready, started) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        publisher.publish(&vec![0; 480], 1, || {
            ready.send(()).unwrap();
            resumed.recv_timeout(Duration::from_secs(2)).unwrap();
            2
        })
    });
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(raw.progress().is_err());
    assert!(raw.accepted_progress().unwrap().frames > 0);
    assert!(supervisor.observe(&runtime, &raw, 10_000_000).is_none());
    assert_eq!(runtime.health.state(), State::Healthy);
    resume.send(()).unwrap();
    producer.join().unwrap().unwrap();
}

#[test]
fn unhealthy_listener_gate_does_not_acquire_encoded_ring_mutex() {
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    use stream_runtime::health::State;
    let (runtime, worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let mut listener = CompressedListener::new(
        &worker.epoch,
        None,
        false,
        ListenerPolicy::Live,
        &mut || Duration::ZERO,
    );
    let _held = worker.epoch.ring.lock().unwrap();
    assert!(runtime
        .health
        .transition(runtime.health.token(), State::Terminal));
    assert!(matches!(
        listener.step(
            &worker.epoch,
            &mut [0; 960],
            &mut || Duration::ZERO,
            &mut || true
        ),
        ListenerStep::End(End::EncoderUnhealthy)
    ));
}
