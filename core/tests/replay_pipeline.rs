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
use esp_shazam_core::{capture_buffer::CaptureBuffer, stream_buffer::OutputFormat};
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
    publisher.publish(&vec![17; 480]).unwrap();
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
fn close_failure_retains_slot_and_epoch_lease_until_actor_is_released() {
    use esp_shazam_core::stream_wait::Slot;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use stream_pipeline::ConnectionResources;
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let slots = Arc::new(AtomicUsize::new(0));
    let resources = ConnectionResources::new((), Slot::take(&slots, 1).unwrap(), lease);
    let retained = resources
        .close(|_| Err::<(), _>("uncertain close"))
        .err()
        .unwrap();
    assert_eq!(slots.load(Ordering::Acquire), 1);
    let request = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    );
    assert!(
        !worker.service(&runtime),
        "retained lease must block idle switch"
    );
    let stream_runtime::SwitchStart::Pending(pending) = request else {
        panic!("pending")
    };
    assert!(matches!(
        pending.poll(),
        stream_runtime::SwitchPoll::Finished(Ok(false))
    ));
    drop(retained);
    assert_eq!(slots.load(Ordering::Acquire), 0);
    let request = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    );
    assert!(
        worker.service(&runtime),
        "released lease permits idle switch"
    );
    drop(request);
}

#[test]
fn pending_header_cannot_be_preempted_and_payload_accounting_requires_complete_write() {
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let old = worker.epoch.clone();
    let mut time = Duration::ZERO;
    let mut listener = CompressedListener::new(&old, None, false, &mut || time);
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
        listener.write_finished(Ok(())),
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
fn retirement_during_fresh_wait_is_fell_behind_and_empty_expiry_is_stalled() {
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    for retire in [false, true] {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let epoch = worker.epoch.clone();
        let mut listener = CompressedListener::new(&epoch, None, false, &mut || Duration::ZERO);
        let mut bytes = vec![0; OutputFormat::Flac.max_frame_bytes()];
        listener.step(&epoch, &mut bytes, &mut || Duration::ZERO, &mut || true);
        listener.step(&epoch, &mut bytes, &mut || Duration::ZERO, &mut || true);
        listener.write_finished(Ok(()));
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
        let end = listener.step(
            &epoch,
            &mut bytes,
            &mut || Duration::from_secs(2),
            &mut || true,
        );
        assert!(matches!(
            (retire, end),
            (true, ListenerStep::End(End::FellBehind))
                | (false, ListenerStep::End(End::CaptureStalled))
        ));
    }
}

#[test]
fn close_callback_runs_once_before_releasing_slot_and_lease() {
    use esp_shazam_core::stream_wait::Slot;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use stream_pipeline::ConnectionResources;
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let slots = Arc::new(AtomicUsize::new(0));
    let resources = ConnectionResources::new((), Slot::take(&slots, 1).unwrap(), lease);
    let mut calls = 0;
    assert_eq!(
        resources
            .close(|_| {
                calls += 1;
                assert_eq!(slots.load(Ordering::Acquire), 1);
                let request = runtime.begin_switch(
                    OutputFormat::Mp3,
                    false,
                    Arc::new(Mutex::new(true)),
                    Duration::from_secs(1),
                );
                assert!(!worker.service(&runtime));
                drop(request);
                Ok::<_, ()>(13)
            })
            .ok(),
        Some((13, 1))
    );
    assert_eq!(calls, 1);
    assert_eq!(slots.load(Ordering::Acquire), 0);
    let request = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(Mutex::new(true)),
        Duration::from_secs(1),
    );
    assert!(worker.service(&runtime));
    drop(request);
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
    publisher.publish(&vec![41; 480]).unwrap();
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

#[test]
fn payload_counters_advance_only_after_a_whole_successful_write() {
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    for successful in [false, true] {
        let (_, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let epoch = worker.epoch.clone();
        let mut listener = CompressedListener::new(&epoch, None, false, &mut || Duration::ZERO);
        let mut scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
        listener.step(&epoch, &mut scratch, &mut || Duration::ZERO, &mut || true);
        assert!(matches!(
            listener.step(&epoch, &mut scratch, &mut || Duration::ZERO, &mut || true),
            ListenerStep::Write { header: true, .. }
        ));
        listener.write_finished(Ok(()));
        worker.encoder.push(&vec![0; 8192]);
        worker.encoder.encode_block(0, &mut worker.bytes).unwrap();
        let payload_len = worker.bytes.len() as u64;
        epoch.ring.lock().unwrap().push(&worker.bytes).unwrap();
        assert!(matches!(
            listener.step(&epoch, &mut scratch, &mut || Duration::ZERO, &mut || true),
            ListenerStep::Write { header: false, .. }
        ));
        assert_eq!(listener.frames_sent, 0);
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
                listener.write_finished(Ok(())),
                ListenerStep::Progress
            ));
            assert_eq!(
                (listener.frames_sent, listener.bytes_sent),
                (1, payload_len)
            );
        } else {
            assert!(matches!(
                listener.write_finished(Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "partial payload expired"
                ))),
                ListenerStep::End(End::StoppedReading)
            ));
            assert_eq!((listener.frames_sent, listener.bytes_sent), (0, 0));
        }
    }
}

#[test]
fn close_reports_slots_held_after_actual_close_before_releasing_its_slot() {
    use esp_shazam_core::stream_wait::Slot;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let slots = Arc::new(AtomicUsize::new(0));
    let own = Slot::take(&slots, 2).unwrap();
    let other = Slot::take(&slots, 2).unwrap();
    let resources = stream_pipeline::ConnectionResources::new((), own, None);
    let result = resources
        .close(|_| {
            assert_eq!(slots.load(Ordering::Acquire), 2);
            // Another close completes while this close is in progress.
            drop(other);
            assert_eq!(slots.load(Ordering::Acquire), 1);
            Ok::<_, ()>(37)
        })
        .ok();
    assert_eq!(result, Some((37, 1)));
    assert_eq!(slots.load(Ordering::Acquire), 0);
}

#[test]
fn cursor_acquisition_reports_the_actual_join_position_before_any_input() {
    use stream_pipeline::EncoderObservation;
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(3840, 240);
    publisher.publish(&vec![31; 480]).unwrap();
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
