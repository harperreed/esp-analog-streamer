// ABOUTME: Replays bounded environment schedules through actual production stream actors.
// ABOUTME: Checks stable traces, source PCM, ownership and independent decoder artifacts.
#![cfg(not(any(feature = "capture-loom", feature = "listener-loom")))]
#[path = "support/replay.rs"]
mod replay;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_codec.rs"]
mod stream_codec;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_pipeline.rs"]
mod stream_pipeline;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_runtime.rs"]
mod stream_runtime;

#[test]
fn explicit_fixtures_repeat_exactly_and_emit_real_artifacts() {
    let names = [
        "delivery",
        "overrun",
        "overrun-live",
        "reset",
        "partial",
        "ownership",
        "absence",
        "production-pause",
        "header-error",
        "disconnect-stale",
        "partial-minimized",
    ];
    if let Some(root) = std::env::var_os("STREAM_REPLAY_OUTPUT") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            std::path::PathBuf::from(root).join("fixture-index.txt"),
            names.join("\n") + "\nreduced\n",
        )
        .unwrap();
    }
    for name in names {
        let fixture = replay::Fixture::load(name);
        let first = replay::run(&fixture);
        let second = replay::run(&fixture);
        assert_eq!(first, second, "fixture {name}");
        first.check(&fixture);
        if let Some(root) = std::env::var_os("STREAM_REPLAY_OUTPUT") {
            first.emit(&std::path::PathBuf::from(root).join(name), &fixture);
        }
    }
}

#[test]
fn reducer_preserves_named_predicate_and_actor_dependencies() {
    let fixture = replay::Fixture::load("partial");
    let reduced = replay::reduce(fixture.clone(), |case| {
        let result = replay::run(case);
        result.has_partial_timeout() && result.delivery_matches(case) && result.source_partition()
    });
    assert!(reduced.events.len() < fixture.events.len());
    assert!(replay::run(&reduced).has_partial_timeout());
    assert!(reduced.valid());
    if let Some(root) = std::env::var_os("STREAM_REPLAY_OUTPUT") {
        let root = std::path::PathBuf::from(root).join("reduced");
        let mut durable = reduced.clone();
        durable.expected = vec!["partial_timeout".into()];
        replay::run(&durable).emit(&root, &durable);
        std::fs::write(
            root.join("original.json"),
            serde_json::to_vec_pretty(&fixture).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn runtime_requests_obey_tokens_expiry_block_boundaries_and_draining_leases() {
    use esp_shazam_core::{
        capture_buffer::CaptureBuffer, stream_buffer::OutputFormat, stream_wait::Slot,
    };
    use std::{
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };
    use stream_pipeline::{ConnectionResources, EncoderPump};
    use stream_runtime::{Runtime, SwitchPoll, SwitchStart};
    let clock = Arc::new(AtomicU64::new(0));
    let shared = clock.clone();
    let (runtime, mut worker) = Runtime::with_clock(
        OutputFormat::Flac,
        false,
        Arc::new(move || Duration::from_micros(shared.load(Ordering::Acquire))),
    )
    .unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(8192, 16);
    let mut pump = EncoderPump::new();
    let mut now = || Duration::from_micros(clock.load(Ordering::Acquire));
    pump.step(&raw, &mut worker, &runtime, &mut now, &mut |_| {});
    publisher.publish(&[12345; 32], 0, || 0).unwrap();
    pump.step(&raw, &mut worker, &runtime, &mut now, &mut |_| {});
    assert!(!pump.block_boundary(&worker));
    let revoked = Arc::new(Mutex::new(false));
    let SwitchStart::Pending(pending) =
        runtime.begin_switch(OutputFormat::Mp3, true, revoked, Duration::from_secs(1))
    else {
        panic!("actual different-format revoked request must enqueue")
    };
    pump.step(&raw, &mut worker, &runtime, &mut now, &mut |_| {});
    let SwitchPoll::Pending(pending) = pending.poll() else {
        panic!("partial block must defer service")
    };
    // Complete the real partial FLAC block. Service remains deferred until its next outer boundary.
    publisher.publish(&vec![23456; 8160], 0, || 0).unwrap();
    for _ in 0..260 {
        pump.step(&raw, &mut worker, &runtime, &mut now, &mut |_| {});
    }
    let SwitchPoll::Finished(Err(cancelled)) = pending.poll() else {
        panic!("pump boundary must service cancelled request")
    };
    assert_eq!(cancelled.to_string(), "encoder switch cancelled");
    assert_eq!(runtime.active_format(), OutputFormat::Flac);
    let SwitchStart::Pending(expired) = runtime.begin_switch(
        OutputFormat::Mp3,
        true,
        Arc::new(Mutex::new(true)),
        Duration::from_micros(10),
    ) else {
        panic!("enqueue")
    };
    clock.store(10, Ordering::Release);
    assert!(matches!(expired.poll(), SwitchPoll::Finished(Err(_))));
    worker.service(&runtime);
    assert_eq!(runtime.active_format(), OutputFormat::Flac);
    let slots = Arc::new(esp_shazam_core::stream_wait::Capacity::default());
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let draining = ConnectionResources::new(1, Slot::take(&slots, 2).unwrap(), lease);
    let SwitchStart::Pending(idle) = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("enqueue")
    };
    worker.service(&runtime);
    assert!(matches!(idle.poll(), SwitchPoll::Finished(Ok(false))));
    assert_eq!(runtime.active_format(), OutputFormat::Flac);
    draining.close(|_| Ok::<_, ()>(())).ok().unwrap();
    let old = worker.epoch.clone();
    publisher.publish(&[22222; 32], 0, || 0).unwrap();
    clock.store(20000, Ordering::Release);
    pump.step(&raw, &mut worker, &runtime, &mut now, &mut |_| {});
    assert!(!pump.block_boundary(&worker));
    let SwitchStart::Pending(force) = runtime.begin_switch(
        OutputFormat::Mp3,
        true,
        Arc::new(Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("enqueue")
    };
    let SwitchPoll::Pending(force) = force.poll() else {
        panic!("valid request must remain pending during partial assembly")
    };
    publisher.publish(&vec![33333; 8160], 0, || 0).unwrap();
    for _ in 0..255 {
        pump.step(&raw, &mut worker, &runtime, &mut now, &mut |_| {});
    }
    assert_eq!(runtime.active_format(), OutputFormat::Flac);
    assert!(pump.block_boundary(&worker));
    assert!(matches!(
        pump.step(&raw, &mut worker, &runtime, &mut now, &mut |_| {}),
        stream_pipeline::EncoderStep::Switched
    ));
    assert!(matches!(force.poll(), SwitchPoll::Finished(Ok(true))));
    assert!(old.retired());
    let (_, lease) = runtime.admit("GET /stream.mp3 HTTP/1.1");
    assert_eq!(lease.unwrap().epoch.format, OutputFormat::Mp3);
    assert_eq!(slots.held(), 0);
}

#[cfg(feature = "capture-fixture")]
#[test]
fn atomic_copy_reset_rejects_scratch_and_new_generation_reconnect_is_exact() {
    use esp_shazam_core::{
        capture_buffer::CaptureBuffer, stream_buffer::OutputFormat, stream_http::ListenerPolicy,
    };
    use std::time::Duration;
    use stream_pipeline::{CompressedListener, EncoderObservation, EncoderPump, End, ListenerStep};
    let (runtime, mut worker) = stream_runtime::Runtime::new(OutputFormat::Flac, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(8192, 16);
    let mut pump = EncoderPump::new();
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |_| {},
    );
    let (_, old_lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let mut old_listener = CompressedListener::new(
        &old_lease.as_ref().unwrap().epoch,
        None,
        false,
        ListenerPolicy::Live,
        &mut || Duration::ZERO,
    );
    let mut old_scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
    old_listener.step(
        &worker.epoch,
        &mut old_scratch,
        &mut || Duration::ZERO,
        &mut || true,
    );
    old_listener.step(
        &worker.epoch,
        &mut old_scratch,
        &mut || Duration::ZERO,
        &mut || true,
    );
    old_listener.write_finished(Ok(()), &mut || Duration::ZERO);
    publisher.publish(&[76543; 32], 0, || 0).unwrap();
    let mut accepted = 0;
    let mut resets = 0;
    pump.step_with_copy_hook(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |obs| match obs {
            EncoderObservation::AcceptedInput { frames, .. } => accepted += frames,
            EncoderObservation::Reset => resets += 1,
            _ => {}
        },
        || {
            publisher.invalidate().unwrap();
        },
    );
    assert_eq!(accepted, 0);
    assert_eq!(resets, 1);
    assert_eq!(worker.encoder.frames_wanted(), 4096);
    // The same raw ring, pump and old listener continue across the rejected copy.
    assert!(matches!(
        old_listener.step(
            &worker.epoch,
            &mut old_scratch,
            &mut || Duration::ZERO,
            &mut || true
        ),
        ListenerStep::End(End::EncodedReset)
    ));
    let (_, new_lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let epoch = &new_lease.as_ref().unwrap().epoch;
    let mut new_listener =
        CompressedListener::new(epoch, None, false, ListenerPolicy::Live, &mut || {
            Duration::ZERO
        });
    let mut scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
    new_listener.step(epoch, &mut scratch, &mut || Duration::ZERO, &mut || true);
    new_listener.step(epoch, &mut scratch, &mut || Duration::ZERO, &mut || true);
    new_listener.write_finished(Ok(()), &mut || Duration::ZERO);
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |_| {},
    );
    let pcm: Vec<i32> = (0..8192).map(|i| ((i * 23 % 60001) - 30000) << 8).collect();
    publisher.publish(&pcm, 0, || 0).unwrap();
    let mut next = 0;
    for _ in 0..256 {
        pump.step(
            &raw,
            &mut worker,
            &runtime,
            &mut || Duration::ZERO,
            &mut |obs| {
                if let EncoderObservation::AcceptedInput {
                    generation,
                    start,
                    frames,
                } = obs
                {
                    assert_eq!(generation, 1);
                    assert_eq!(start, next);
                    next += frames as u64;
                }
            },
        );
    }
    assert_eq!(next, 4096);
    assert!(matches!(
        new_listener.step(
            &worker.epoch,
            &mut scratch,
            &mut || Duration::ZERO,
            &mut || true
        ),
        ListenerStep::Write { header: false, .. }
    ));
    let frame = new_listener.write_bytes(&scratch);
    assert_eq!(frame[4], 0);
    let mut flac = stream_codec::stream_header(OutputFormat::Flac);
    flac.extend_from_slice(frame);
    new_listener.write_finished(Ok(()), &mut || Duration::ZERO);
    let decoded: Vec<_> = claxon::FlacReader::new(std::io::Cursor::new(flac))
        .unwrap()
        .samples()
        .map(|s| s.unwrap())
        .collect();
    assert_eq!(
        decoded,
        pcm.iter()
            .map(|s| (s >> 8).clamp(-32768, 32767))
            .collect::<Vec<_>>()
    );
    assert_eq!(new_listener.frames_sent, 1);
}

#[test]
fn absent_capture_before_buffered_admission_expires_without_any_pcm() {
    use esp_shazam_core::{
        stream_buffer::{BufferPreset, OutputFormat},
        stream_http::ListenerPolicy,
    };
    use std::time::Duration;
    use stream_pipeline::{CompressedListener, End, ListenerStep};
    let (runtime, worker) = stream_runtime::Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let epoch = &lease.as_ref().unwrap().epoch;
    let mut listener = CompressedListener::new(
        epoch,
        Some(BufferPreset::Low),
        false,
        ListenerPolicy::Live,
        &mut || Duration::ZERO,
    );
    let mut scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
    assert!(matches!(
        listener.step(epoch, &mut scratch, &mut || Duration::ZERO, &mut || true),
        ListenerStep::SleepUntil(_)
    ));
    assert!(matches!(
        listener.step(
            epoch,
            &mut scratch,
            &mut || Duration::from_secs(2),
            &mut || true
        ),
        ListenerStep::End(End::CaptureStalled)
    ));
    assert!(listener.refuse_buffered);
    assert_eq!(listener.frames_sent, 0);
    assert_eq!(listener.bytes_sent, 0);
    assert_eq!(worker.encoder.frames_wanted(), 4096);
}

#[test]
fn source_and_actor_receipts_reject_real_accounting_omission_and_duplication() {
    let fixture = replay::Fixture::load("delivery");
    let good = replay::run(&fixture);
    let mut skipped_input = good.clone();
    skipped_input.input_reads.remove(0);
    assert!(!skipped_input.source_partition(), "source_partition");
    let mut repeated_input = good.clone();
    repeated_input
        .input_reads
        .insert(0, repeated_input.input_reads[0].clone());
    assert!(!repeated_input.source_partition(), "source_partition");
    let mut omitted = good.clone();
    omitted.listeners.get_mut(&0).unwrap().ranges.remove(0);
    assert!(
        !omitted.delivery_matches(&fixture),
        "delivery_source_sequence"
    );
    let mut duplicated = good.clone();
    let range = duplicated.listeners[&0].ranges[0];
    duplicated
        .listeners
        .get_mut(&0)
        .unwrap()
        .ranges
        .insert(0, range);
    assert!(
        !duplicated.delivery_matches(&fixture),
        "delivery_source_sequence"
    );
}
