// ABOUTME: Exercises encoder owner publication without holding producer resources during export.
// ABOUTME: Uses production queues, probe phases and encoder turn scheduling.
#![cfg(not(any(feature = "capture-loom", feature = "listener-loom")))]
#[allow(dead_code)]
#[path = "../../firmware/src/listener_telemetry.rs"]
mod listener_telemetry;
#[allow(dead_code)]
mod stream_codec {
    pub fn release_mp3_codec(encoder: &mut Encoder) {
        let Encoder::Mp3(mp3) = encoder else {
            panic!("actual MP3 owner required")
        };
        drop(mp3.codec.take());
    }
    include!("../../firmware/src/stream_codec.rs");
}
#[cfg(feature = "encoder-probe")]
#[path = "../../firmware/src/encoder_probe.rs"]
mod encoder_probe;
#[allow(dead_code)]
#[path = "../../firmware/src/encoder_telemetry.rs"]
mod encoder_telemetry;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_pipeline.rs"]
mod stream_pipeline;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_runtime.rs"]
mod stream_runtime;
use encoder_telemetry::{EncoderMetrics, Payload, Queue, Sample};
use esp_shazam_core::stream_buffer::OutputFormat;
use std::sync::{Arc, Barrier};
fn sample(us: u64) -> Sample {
    Sample {
        us,
        runtime: (us / 2) as u32,
        listeners: 0,
        stack_free_bytes: 1234,
    }
}
#[test]
fn delayed_consumer_releases_queue_before_owner_reports() {
    let queue = Arc::new(Queue::new());
    let mut owner = EncoderMetrics::new(sample(100));
    owner.frames = 7;
    owner.raw_read.calls = 3;
    assert!(owner.report(&queue, sample(10_000_100), OutputFormat::Mp3));
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let consumer = {
        let q = queue.clone();
        let e = entered.clone();
        let r = release.clone();
        std::thread::spawn(move || {
            let record = q.take().unwrap();
            e.wait();
            r.wait();
            record
        })
    };
    entered.wait();
    owner.frames = 9;
    assert!(owner.boundary(
        &queue,
        encoder_telemetry::Reason::Overrun,
        sample(10_000_200),
        OutputFormat::Mp3,
        #[cfg(feature = "encoder-probe")]
        None
    ));
    assert!(owner.report(&queue, sample(20_000_100), OutputFormat::Mp3));
    assert!(!owner.report(&queue, sample(30_000_100), OutputFormat::Mp3));
    assert!(!owner.report(&queue, sample(40_000_100), OutputFormat::Mp3));
    assert_eq!(owner.raw_read.calls, 0);
    assert_eq!(owner.since_us, 40_000_100);
    release.wait();
    let Payload::Window(window) = consumer.join().unwrap().payload else {
        panic!("window")
    };
    assert_eq!(
        (window.start_us, window.end_us, window.window, window.epoch),
        (100, 10_000_100, 1, 0)
    );
    assert_eq!(
        (
            window.frames,
            window.raw_read.calls,
            window.stack_free_bytes,
            window.task_runtime_us
        ),
        (7, 3, 1234, 5_000_000)
    );
    assert!(matches!(
        queue.take().unwrap().payload,
        Payload::Boundary(_)
    ));
    let Payload::Window(second) = queue.take().unwrap().payload else {
        panic!("window")
    };
    assert_eq!(
        (second.window, second.frames, second.raw_read.calls),
        (2, 9, 0)
    );
    assert_eq!(queue.dropped_full(), 2);
    assert!(queue.loss_occurred());
}
#[test]
fn encoder_turn_preserves_cfg_specific_order_and_eight_publication_delay() {
    let mut publications = 0;
    let mut calls = Vec::new();
    for i in 0..8 {
        stream_pipeline::finish_encoder_turn(
            true,
            OutputFormat::Mp3,
            &mut publications,
            &mut calls,
            |calls| calls.push((i, "report")),
            |calls| calls.push((i, "delay")),
        );
    }
    assert_eq!(calls.len(), 9);
    #[cfg(not(feature = "encoder-probe"))]
    assert_eq!(&calls[7..], &[(7, "report"), (7, "delay")]);
    #[cfg(feature = "encoder-probe")]
    assert_eq!(&calls[7..], &[(7, "delay"), (7, "report")]);
}
#[cfg(feature = "encoder-probe")]
#[test]
fn full_queue_cannot_change_actual_probe_phase_progression() {
    let queue = Queue::new();
    let mut owner = EncoderMetrics::new(sample(0));
    owner.probe.restart(true);
    for i in 0..24 {
        assert_eq!(owner.probe.enabled(), (8..16).contains(&i));
        owner.raw_read.wait_total_us = 20;
        owner.encode_wall_total_us = 30;
        let published = owner.report(&queue, sample((i + 1) * 10_000_000), OutputFormat::Mp3);
        assert_eq!(published, i < 2);
        assert_eq!(owner.raw_read.wait_total_us, 0);
    }
    assert_eq!(owner.probe.position(), (1, "COMPLETE", 1));
    let Payload::Window(first) = queue.take().unwrap().payload else {
        panic!("window")
    };
    assert_eq!(
        (
            first.probe.sequence,
            first.probe.phase,
            first.probe.interval,
            first.probe.residual_us
        ),
        (1, "OFF1", 1, Some(9_999_950))
    );
}
#[test]
fn replacement_preserves_existing_cfg_counter_policy_and_checked_identity() {
    use encoder_telemetry::Reason;
    let q = Queue::new();
    let mut owner = EncoderMetrics::new(sample(100));
    owner.frames = 12;
    owner.resets = 3;
    owner.raw_read.overruns = 4;
    assert!(owner.boundary(
        &q,
        Reason::Switched,
        sample(200),
        OutputFormat::Mp3,
        #[cfg(feature = "encoder-probe")]
        None
    ));
    let Payload::Boundary(boundary) = q.take().unwrap().payload else {
        panic!("boundary")
    };
    assert_eq!(
        (
            boundary.frames,
            boundary.resets,
            boundary.raw_overruns,
            boundary.baseline_us
        ),
        (12, 3, 4, 200)
    );
    assert_eq!(owner.epoch, 1);
    #[cfg(not(feature = "encoder-probe"))]
    assert_eq!((owner.frames, owner.resets), (0, 0));
    #[cfg(feature = "encoder-probe")]
    assert_eq!(
        (
            owner.frames,
            owner.resets,
            boundary.sequence,
            boundary.position.0
        ),
        (12, 3, 1, 0)
    );
    owner.epoch = u64::MAX;
    assert!(!owner.boundary(
        &q,
        Reason::Switched,
        sample(300),
        OutputFormat::Mp3,
        #[cfg(feature = "encoder-probe")]
        None
    ));
    assert!(q.loss_occurred());
    assert!(q.take().is_none());
}
#[test]
fn overrun_preserves_default_interval_and_refreshes_probe_interval() {
    use encoder_telemetry::Reason;
    let q = Queue::new();
    let mut owner = EncoderMetrics::new(sample(100));
    owner.resets = 1;
    owner.raw_read.overruns = 1;
    owner.boundary(
        &q,
        Reason::Overrun,
        sample(300),
        OutputFormat::Mp3,
        #[cfg(feature = "encoder-probe")]
        None,
    );
    #[cfg(not(feature = "encoder-probe"))]
    assert_eq!((owner.since_us, owner.raw_read.overruns), (100, 1));
    #[cfg(feature = "encoder-probe")]
    assert_eq!((owner.since_us, owner.raw_read.overruns), (300, 0));
}

#[test]
fn actual_pump_reset_switch_and_terminal_codec_failure_reach_owner_boundary() {
    use encoder_telemetry::{boundary_reason, Reason};
    use esp_shazam_core::capture_buffer::CaptureBuffer;
    use std::{sync::Mutex, time::Duration};
    use stream_pipeline::{EncoderObservation, EncoderPump, EncoderStep};
    let (runtime, mut worker) = stream_runtime::Runtime::new(OutputFormat::Flac, false).unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(8192, 16);
    let q = Queue::new();
    let mut owner = EncoderMetrics::new(sample(0));
    let mut pump = EncoderPump::new();
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |_| {},
    );
    publisher.invalidate().unwrap();
    let mut reset = false;
    let step = pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |observation| {
            if let EncoderObservation::Reset = observation {
                owner.observe_reset();
                reset = true;
            }
        },
    );
    let reason = boundary_reason(&step, reset).unwrap();
    assert_eq!(reason, Reason::Overrun);
    owner.boundary(
        &q,
        reason,
        sample(100),
        worker.epoch.format,
        #[cfg(feature = "encoder-probe")]
        None,
    );
    let Payload::Boundary(boundary) = q.take().unwrap().payload else {
        panic!("boundary")
    };
    assert_eq!(boundary.resets, 1);
    let stream_runtime::SwitchStart::Pending(pending) = runtime.begin_switch(
        OutputFormat::Mp3,
        true,
        Arc::new(Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("pending")
    };
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |_| {},
    );
    let step = pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |_| {},
    );
    let reason = boundary_reason(&step, false).unwrap();
    assert_eq!(reason, Reason::Switched);
    assert!(matches!(
        pending.poll(),
        stream_runtime::SwitchPoll::Finished(Ok(_))
    ));
    owner.boundary(
        &q,
        reason,
        sample(200),
        worker.epoch.format,
        #[cfg(feature = "encoder-probe")]
        None,
    );
    assert_eq!(q.take().unwrap().epoch, 1);
    pump.step(
        &raw,
        &mut worker,
        &runtime,
        &mut || Duration::ZERO,
        &mut |_| {},
    );
    stream_codec::release_mp3_codec(&mut worker.encoder);
    publisher.publish(&vec![1234; 2 * 1152], 0, || 0).unwrap();
    // The fixture commits sixteen-frame capture blocks: one MP3 block needs 72 reads.
    let step = (0..96)
        .find_map(|_| {
            let step = pump.step(
                &raw,
                &mut worker,
                &runtime,
                &mut || Duration::ZERO,
                &mut |_| {},
            );
            matches!(step, EncoderStep::Stopped(_)).then_some(step)
        })
        .expect("one actual MP3 block must reach the terminal codec error within 96 pump turns");
    assert_eq!(boundary_reason(&step, false), Some(Reason::Stopped));
    owner.boundary(
        &q,
        Reason::Stopped,
        sample(300),
        worker.epoch.format,
        #[cfg(feature = "encoder-probe")]
        None,
    );
    let EncoderStep::Stopped(error) = step else {
        unreachable!()
    };
    owner.failure(&q, 301, &error);
    drop(error);
    drop(worker);
    drop(pump);
    drop(raw);
    drop(publisher);
    assert!(matches!(q.take().unwrap().payload, Payload::Boundary(_)));
    let Payload::Failure(code) = q.take().unwrap().payload else {
        panic!("numeric error")
    };
    assert_eq!(code.kind, std::io::ErrorKind::Other);
}
#[cfg(feature = "encoder-probe")]
#[test]
fn exhausted_window_identity_still_advances_owner_probe_once() {
    let q = Queue::new();
    let mut owner = EncoderMetrics::new(sample(0));
    owner.probe.restart(true);
    owner.window = u64::MAX;
    assert!(!owner.report(&q, sample(10_000_000), OutputFormat::Mp3));
    assert_eq!(owner.probe.position(), (1, "OFF1", 2));
    assert_eq!(owner.since_us, 10_000_000);
    assert!(q.loss_occurred());
    assert!(q.take().is_none());
}
#[cfg(feature = "encoder-probe")]
#[test]
fn successful_owner_windows_keep_probe_labels_and_checked_nested_spans() {
    let q = Queue::new();
    let mut owner = EncoderMetrics::new(sample(0));
    owner.probe.restart(true);
    for i in 0..24 {
        let on = (8..16).contains(&i);
        owner.raw_read.wait_total_us = 20;
        owner.encode_wall_total_us = 30;
        if on {
            owner.probe.attempt(true, 7);
            owner.probe.attempt(false, 4);
            owner.probe.push(5);
            owner.probe.delay(9);
        }
        assert!(owner.report(&q, sample((i + 1) * 10_000_000), OutputFormat::Mp3));
        let Payload::Window(w) = q.take().unwrap().payload else {
            panic!("window")
        };
        let r = w.probe;
        assert_eq!(
            (r.phase, r.interval, r.retained, r.completed),
            (
                ["OFF1", "ON", "OFF2"][(i / 8) as usize],
                (i % 8 + 1) as u8,
                i % 8 > 1,
                i == 23
            )
        );
        assert_eq!(
            (r.raw_remainder_us, r.residual_us),
            if on {
                (Some(9), Some(9_999_936))
            } else {
                (Some(20), Some(9_999_950))
            }
        );
        assert_eq!(
            (
                r.success_us,
                r.rejected_us,
                r.push_us,
                r.delay_us,
                r.delay_calls
            ),
            if on { (7, 4, 5, 9, 1) } else { (0, 0, 0, 0, 0) }
        );
        assert!(r.valid);
    }
}

#[test]
fn fixed_record_and_queue_layout_is_reported_for_parent_target_comparison() {
    fn copy_value<T: Copy>() {}
    copy_value::<encoder_telemetry::Record>();
    println!(
        "host record_bytes={} queue_bytes={}",
        std::mem::size_of::<encoder_telemetry::Record>(),
        std::mem::size_of::<Queue>()
    );
}
#[cfg(feature = "encoder-probe")]
#[test]
fn replacement_storage_copies_actual_receipts_before_owners_expire() {
    use encoder_telemetry::{Reason, Storage};
    use esp_shazam_core::capture_buffer::CaptureBuffer;
    let (_runtime, worker) = stream_runtime::Runtime::new(OutputFormat::Mp3, false).unwrap();
    let (publisher, raw) = CaptureBuffer::new(8192, 16);
    let pump = stream_pipeline::EncoderPump::new();
    let expected = worker.encoder.mp3_storage_receipt().unwrap();
    let storage = Storage::snapshot(&raw, &pump, &worker);
    assert_eq!(
        storage.raw_payload,
        (raw.payload_range().0 as usize, raw.payload_range().1)
    );
    assert_eq!(
        storage.raw_stamps,
        (raw.stamps_range().0 as usize, raw.stamps_range().1)
    );
    assert_eq!(storage.chunk.1, 1920);
    assert_eq!(
        (storage.output_capacity, storage.output_required),
        (960, 960)
    );
    let q = Queue::new();
    let mut owner = EncoderMetrics::new(sample(0));
    owner.boundary(
        &q,
        Reason::Startup,
        sample(100),
        OutputFormat::Mp3,
        Some(storage),
    );
    drop(worker);
    drop(pump);
    drop(raw);
    drop(publisher);
    let Payload::Boundary(b) = q.take().unwrap().payload else {
        panic!("boundary")
    };
    assert_eq!(b.storage.unwrap().mp3.unwrap(), expected);
    assert_eq!((b.sequence, b.baseline_us), (1, 100));
}
#[test]
fn startup_initialization_preserves_existing_records_and_sticky_loss() {
    let queue = Queue::new();
    let mut owner = EncoderMetrics::new(sample(0));
    assert!(owner.report(&queue, sample(10_000_000), OutputFormat::Mp3));
    assert!(owner.report(&queue, sample(20_000_000), OutputFormat::Mp3));
    assert!(!owner.report(&queue, sample(30_000_000), OutputFormat::Mp3));
    queue.initialize();
    assert_eq!(
        (queue.dropped(), queue.dropped_full(), queue.loss_occurred()),
        (1, 1, true)
    );
    assert_eq!(queue.take().unwrap().window, 1);
    assert_eq!(queue.take().unwrap().window, 2);
    assert!(queue.take().is_none());
}
