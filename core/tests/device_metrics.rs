// ABOUTME: Exercises encoder owner publication without holding producer resources during export.
// ABOUTME: Uses production queues, probe phases and encoder turn scheduling.
#![cfg(not(any(feature = "capture-loom", feature = "listener-loom")))]
#[allow(dead_code)]
#[path = "../../firmware/src/listener_observation.rs"]
mod listener_observation;
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
#[allow(dead_code)]
#[path = "../../firmware/src/device_metrics.rs"]
mod device_metrics;
#[allow(dead_code)]
#[path = "../../firmware/src/device_metrics_identity.rs"]
mod device_metrics_identity;
#[cfg(feature = "encoder-probe")]
#[allow(dead_code)]
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
use device_metrics::*;
#[test]
fn startup_is_unavailable() {
    let cache = Cache::new();
    let json: serde_json::Value = serde_json::from_slice(
        &prepare_response(
            &cache,
            || 10,
            None,
            None,
            None,
            None,
            || [listener_observation::Observation::default(); 2],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(json["schema_version"], 1);
    for key in [
        "capture",
        "encoder",
        "listener_loss",
        "encoder_loss",
        "boot_token",
        "build_elf_sha256",
        "settings",
    ] {
        assert!(json[key].is_null(), "{key}");
    }
}
#[test]
fn polling_preserves_acquisition_times() {
    let cache = Cache::new();
    cache.publish_listener_loss(Loss {
        observed_device_us: 3,
        dropped: 0,
        loss_occurred: false,
        metrics_stack_free_bytes: Some(0),
    });
    for served in [10, 20] {
        let json: serde_json::Value = serde_json::from_slice(
            &prepare_response(
                &cache,
                || served,
                None,
                None,
                None,
                None,
                || [listener_observation::Observation::default(); 2],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(json["served_device_us"], served);
        assert_eq!(json["listener_loss"]["observed_device_us"], 3);
        assert_eq!(json["listener_loss"]["metrics_stack_free_bytes"], 0);
        assert!(json["encoder_loss"].is_null());
    }
}
#[test]
fn cache_sizes_fit() {
    assert!(std::mem::size_of::<Cache>() <= 768);
    assert!(std::mem::size_of::<Snapshot>() <= 768);
}

fn window() -> encoder_telemetry::Record {
    let queue = encoder_telemetry::Queue::new();
    let mut owner = encoder_telemetry::EncoderMetrics::new(encoder_telemetry::Sample {
        us: 1,
        runtime: 1,
        listeners: 2,
        stack_free_bytes: 0,
    });
    owner.frames = 9;
    owner.raw_read.frames = 7;
    owner.raw_read.overruns = 3;
    owner.raw_read.retries = 4;
    owner.report(
        &queue,
        encoder_telemetry::Sample {
            us: 10_000_001,
            runtime: 1,
            listeners: 3,
            stack_free_bytes: 0,
        },
        esp_shazam_core::stream_buffer::OutputFormat::Mp3,
    );
    queue.take().unwrap()
}
#[test]
fn invalid_runtime_and_unscanned_stack_are_null() {
    let mut record = window();
    if let encoder_telemetry::Payload::Window(w) = &mut record.payload {
        w.runtime_valid = false;
        w.task_runtime_us = 99;
    }
    let value = serde_json::to_value(project_encoder(&record).unwrap()).unwrap();
    assert!(value["task_runtime_us"].is_null());
    assert_eq!(value["stack_free_bytes"], 0);
    assert_eq!(value["raw_read_frames"], 7);
    assert_eq!(value["raw_read_overruns"], 3);
    assert_eq!(value["raw_read_retries"], 4);
    if let encoder_telemetry::Payload::Window(w) = &mut record.payload {
        w.runtime_valid = true;
        w.task_runtime_us = 0;
    }
    assert_eq!(
        serde_json::to_value(project_encoder(&record).unwrap()).unwrap()["task_runtime_us"],
        0
    );
}
#[test]
fn window_boundary_failure_replace_in_order() {
    let cache = Cache::new();
    let mut record = window();
    cache.publish_encoder(project_encoder(&record).unwrap());
    for reason in [
        encoder_telemetry::Reason::Startup,
        encoder_telemetry::Reason::Overrun,
        encoder_telemetry::Reason::Switched,
        encoder_telemetry::Reason::Stopped,
    ] {
        record.payload = encoder_telemetry::Payload::Boundary(encoder_telemetry::Boundary {
            prior_epoch: 1,
            reason,
            frames: 99,
            resets: 0,
            raw_overruns: 0,
            raw_wait_us: 0,
            raw_attempts: 0,
            baseline_us: 4,
            format: esp_shazam_core::stream_buffer::OutputFormat::Mp3,
            #[cfg(feature = "encoder-probe")]
            position: (0, "test", 0),
            #[cfg(feature = "encoder-probe")]
            sequence: 0,
            #[cfg(feature = "encoder-probe")]
            storage: None,
        });
        cache.publish_encoder(project_encoder(&record).unwrap());
        let value = serde_json::to_value(cache.snapshot().unwrap()).unwrap();
        assert_eq!(value["encoder"]["kind"], "boundary");
        assert!(value["encoder"].get("encoded_frames").is_none());
        assert!(value["encoder"].get("stack_free_bytes").is_none());
    }
    record.payload = encoder_telemetry::Payload::Failure(listener_telemetry::ErrorCode::from(
        &std::io::Error::other("fixture"),
    ));
    cache.publish_encoder(project_encoder(&record).unwrap());
    let value = serde_json::to_value(cache.snapshot().unwrap()).unwrap();
    assert_eq!(value["encoder"]["kind"], "failure");
    assert!(value["encoder"].get("window_end_us").is_none());
}

fn maximal_capture() -> Capture {
    Capture {
        capture_begin_us: u64::MAX,
        capture_end_us: u64::MAX,
        read_frames: u32::MAX,
        callbacks: u32::MAX,
        queue_overflows: u32::MAX,
        software_dropped_frames: u32::MAX,
        read_errors: u32::MAX,
        restarts: u32::MAX,
        supervisor_restarts: u32::MAX,
        overflow_restarts: u32::MAX,
        settling_discarded_frames: u32::MAX,
        discontinuities: u32::MAX,
        health_restart_requests: u32::MAX,
        read_service_delay_estimate_max_us: u32::MAX,
        raw_work_max_us: u32::MAX,
        meter_work_max_us: u32::MAX,
        store_work_max_us: u32::MAX,
        raw_read_max_us: u32::MAX,
        internal_free: usize::MAX,
        internal_largest: usize::MAX,
        internal_min_sum: usize::MAX,
        psram_free: usize::MAX,
        psram_largest: usize::MAX,
        psram_min_sum: usize::MAX,
    }
}
#[test]
fn capture_fields_keep_units_and_zero_wraps() {
    let cache = Cache::new();
    let capture = maximal_capture();
    cache.publish_capture(capture);
    let json: serde_json::Value = serde_json::from_slice(
        &prepare_response(
            &cache,
            || 1,
            None,
            None,
            None,
            None,
            || [listener_observation::Observation::default(); 2],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(json["capture"], serde_json::to_value(capture).unwrap());
    let mut zero = capture;
    zero.read_frames = 0;
    cache.publish_capture(zero);
    assert_eq!(cache.snapshot().unwrap().capture.unwrap().read_frames, 0);
}
#[test]
fn maximal_response_fits() {
    let cache = Cache::new();
    let maximal_listeners = [listener_observation::Observation {
        generation: u32::MAX,
        state: listener_observation::State::Writing,
        sample_us: u64::MAX,
        media_age_us: Some(u64::MAX),
        write_us: Some(u64::MAX),
        skips: Some(u64::MAX),
        frames_skipped: Some(u64::MAX),
        last_skip_us: Some(u64::MAX),
    }; 2];
    cache.publish_capture(maximal_capture());
    cache.publish_media_progress(maximal_stage());
    cache.publish_last_listener_departure(maximal_departure());
    cache.publish_encoder(EncoderObservation::Window(EncoderWindow {
        window_start_us: u64::MAX,
        window_end_us: u64::MAX,
        telemetry_epoch: u64::MAX,
        telemetry_window: u64::MAX,
        encoded_frames: u64::MAX,
        resets: u64::MAX,
        encode_max_us: u64::MAX,
        raw_read_frames: u64::MAX,
        raw_read_overruns: u64::MAX,
        raw_read_retries: u64::MAX,
        encode_wall_total_us: u64::MAX,
        raw_wait_total_us: u64::MAX,
        raw_wait_max_us: u64::MAX,
        encoded_lock_wait_wall_total_us: u64::MAX,
        encoded_push_held_wall_total_us: u64::MAX,
        block_samples: usize::MAX,
        listeners_start: usize::MAX,
        listeners_end: usize::MAX,
        stack_free_bytes: Some(u32::MAX),
        task_runtime_us: Some(u32::MAX),
        runtime_valid: true,
    }));
    let loss = Loss {
        observed_device_us: u64::MAX,
        dropped: u32::MAX,
        loss_occurred: true,
        metrics_stack_free_bytes: Some(u32::MAX),
    };
    cache.publish_listener_loss(loss);
    cache.publish_encoder_loss(loss);
    let body = prepare_response(
        &cache,
        || u64::MAX,
        Some(device_metrics_identity::Identity::new(
            [254; 16],
            Some([254; 32]),
        )),
        Some(Settings {
            active_format: esp_shazam_core::stream_buffer::OutputFormat::Flac,
            next_play_format: esp_shazam_core::stream_buffer::OutputFormat::Flac,
            next_play_buffer: esp_shazam_core::stream_buffer::BufferPreset::High,
            buffer_available: false,
            autoplay: false,
            recognition_enabled: false,
            pga_gain: u8::MAX,
        }),
        Some(NetworkMetadata {
            generation: u32::MAX,
            connected: false,
            disconnects: u32::MAX,
        }),
        Some(u32::MAX),
        || maximal_listeners,
    )
    .unwrap();
    eprintln!(
        "maximal body={} Snapshot={} Cache={}",
        body.len(),
        std::mem::size_of::<Snapshot>(),
        std::mem::size_of::<Cache>()
    );
    // The measured maximum is 4383 (firmware/src/device_metrics.rs MAX_BODY_BYTES); a silent
    // drift into the last 128 bytes of the cap fails here before it fails on the board.
    assert!(
        body.len() + 128 <= MAX_BODY_BYTES,
        "maximal body {} within 128 of {MAX_BODY_BYTES}",
        body.len()
    );
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["capture"]["read_frames"], u32::MAX);
}
#[test]
fn health_record_does_not_replace_encoder_window() {
    let record = encoder_telemetry::Record {
        event_us: 2,
        epoch: 3,
        window: 4,
        payload: encoder_telemetry::Payload::Health(encoder_telemetry::HealthTransition {
            transition: stream_runtime::health::State::Healthy,
            operator_epoch: 1,
            raw_valid: false,
            encoded_valid: false,
            raw_generation: 0,
            raw_committed: 0,
            encoded_generation: 0,
            encoded_sequence: 0,
            source_end: 0,
            encoded_production_us: 0,
        }),
    };
    assert!(project_encoder(&record).is_none());
}

#[test]
fn loss_receipts_are_independent() {
    let cache = Cache::new();
    let listener = Loss {
        observed_device_us: 4,
        dropped: 0,
        loss_occurred: false,
        metrics_stack_free_bytes: None,
    };
    let encoder = Loss {
        observed_device_us: 9,
        dropped: u32::MAX,
        loss_occurred: true,
        metrics_stack_free_bytes: Some(0),
    };
    assert!(cache.publish_listener_loss(listener));
    assert!(cache.publish_encoder_loss(encoder));
    let snapshot = cache.snapshot().unwrap();
    assert_eq!(snapshot.listener_loss, Some(listener));
    assert_eq!(snapshot.encoder_loss, Some(encoder));
    let json: serde_json::Value = serde_json::from_slice(
        &prepare_response(
            &cache,
            || 20,
            None,
            None,
            None,
            None,
            || [listener_observation::Observation::default(); 2],
        )
        .unwrap(),
    )
    .unwrap();
    assert!(json["listener_loss"]["metrics_stack_free_bytes"].is_null());
    assert_eq!(json["encoder_loss"]["metrics_stack_free_bytes"], 0);
}

#[test]
fn response_identity_and_exact_settings() {
    use esp_shazam_core::stream_buffer::{BufferPreset, OutputFormat};
    let cache = Cache::new();
    let identity = device_metrics_identity::Identity::new([0xab; 16], Some([0xcd; 32]));
    for (format, wire) in [(OutputFormat::Flac, "flac"), (OutputFormat::Mp3, "mp3")] {
        for (buffer, buffer_wire) in [
            (BufferPreset::Low, "low"),
            (BufferPreset::Medium, "med"),
            (BufferPreset::High, "high"),
        ] {
            let settings = Settings {
                active_format: format,
                next_play_format: OutputFormat::Mp3,
                next_play_buffer: buffer,
                buffer_available: false,
                autoplay: true,
                recognition_enabled: false,
                pga_gain: 255,
            };
            let value: serde_json::Value = serde_json::from_slice(
                &prepare_response(
                    &cache,
                    || 10,
                    Some(identity),
                    Some(settings),
                    None,
                    None,
                    || [listener_observation::Observation::default(); 2],
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(value["boot_token"], "ab".repeat(16));
            assert_eq!(value["build_elf_sha256"], "cd".repeat(32));
            assert!(value.get("build_attestation").is_none());
            assert_eq!(
                value["settings"],
                serde_json::json!({"active_format":wire,"next_play_format":"mp3","next_play_buffer":buffer_wire,"buffer_available":false,"autoplay":true,"recognition_enabled":false,"pga_gain":255})
            );
        }
    }
    let unknown = device_metrics_identity::Identity::new([1; 16], None);
    let value: serde_json::Value = serde_json::from_slice(
        &prepare_response(
            &cache,
            || 20,
            Some(unknown),
            None,
            None,
            None,
            || [listener_observation::Observation::default(); 2],
        )
        .unwrap(),
    )
    .unwrap();
    assert!(value["build_elf_sha256"].is_null());
    assert!(value["settings"].is_null());
}

#[test]
fn served_clock_follows_snapshot_and_runs_without_cache_guard() {
    let cache = Cache::new();
    let old = Loss {
        observed_device_us: 10,
        dropped: 1,
        loss_occurred: false,
        metrics_stack_free_bytes: None,
    };
    assert!(cache.publish_listener_loss(old));
    let calls = std::cell::Cell::new(0);
    let body = prepare_response(
        &cache,
        || {
            calls.set(calls.get() + 1);
            assert!(cache.publish_listener_loss(Loss {
                observed_device_us: 30,
                dropped: 2,
                ..old
            }));
            20
        },
        None,
        None,
        None,
        None,
        || [listener_observation::Observation::default(); 2],
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(value["served_device_us"], 20);
    assert_eq!(value["listener_loss"]["observed_device_us"], 10);
    assert_eq!(value["listener_loss"]["dropped"], 1);
    assert_eq!(
        cache
            .snapshot()
            .unwrap()
            .listener_loss
            .unwrap()
            .observed_device_us,
        30
    );
}

#[test]
fn stage_and_completed_departure_families_start_null_and_keep_timestamps() {
    let cache = Cache::new();
    let empty: serde_json::Value = serde_json::from_slice(
        &prepare_response(
            &cache,
            || 20,
            None,
            None,
            None,
            None,
            || [listener_observation::Observation::default(); 2],
        )
        .unwrap(),
    )
    .unwrap();
    assert!(empty["media_progress"].is_null());
    assert!(empty["last_listener_departure"].is_null());
    let stage = project_media_progress(
        10,
        3,
        stream_runtime::health::State::Healthy,
        None,
        None,
        None,
        None,
    );
    assert!(cache.publish_media_progress(stage));
    assert!(cache.publish_last_listener_departure(project_departure(12, 0, None)));
    for served in [20, 30] {
        let value: serde_json::Value = serde_json::from_slice(
            &prepare_response(
                &cache,
                || served,
                None,
                None,
                None,
                None,
                || [listener_observation::Observation::default(); 2],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["media_progress"]["observed_device_us"], 10);
        assert_eq!(value["media_progress"]["health"], "healthy");
        for key in ["accepted", "raw", "encoded", "backlog_frames"] {
            assert!(value["media_progress"][key].is_null());
        }
        assert_eq!(value["last_listener_departure"]["observed_device_us"], 12);
        assert!(value["last_listener_departure"]["stats"].is_null());
    }
}

#[test]
fn stage_projection_preserves_independent_generations_and_acquisition_times() {
    use esp_shazam_core::{
        flac_ring::{FrameMetadata, FrameReceipt, SourceMetadata},
        stream_buffer::OutputFormat,
        stream_ring::{AcceptedInput, RawProgress},
    };
    let accepted = AcceptedInput {
        generation: 2,
        frames: 7,
        at_us: Some(11),
    };
    let raw = RawProgress {
        generation: 3,
        accepted_frames: 8,
        accepted_at_us: Some(12),
        committed_frames: 9,
        committed_at_us: Some(13),
    };
    let output = stream_runtime::health::OutputProgress {
        operator_epoch: 4,
        format: OutputFormat::Mp3,
        receipt: FrameReceipt {
            generation: 5,
            sequence: 6,
            metadata: FrameMetadata {
                source: SourceMetadata {
                    raw_generation: 1,
                    source_start: 2,
                    source_end: 3,
                    oldest_accepted_us: 10,
                    raw_publication_us: 14,
                },
                encoded_production_us: 15,
            },
        },
    };
    for (health, wire) in [
        (stream_runtime::health::State::Healthy, "healthy"),
        (
            stream_runtime::health::State::ResetRequested,
            "reset_requested",
        ),
        (stream_runtime::health::State::Recovering, "recovering"),
        (stream_runtime::health::State::Terminal, "terminal"),
    ] {
        let stage = project_media_progress(
            20,
            8,
            health,
            Some(&accepted),
            Some(&raw),
            Some(&output),
            None,
        );
        let value = serde_json::to_value(stage).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"observed_device_us":20,"operator_epoch":8,"health":wire,"accepted":{"generation":2,"frames":7,"observed_device_us":11},"raw":{"generation":3,"committed_frames":9,"publication_device_us":13},"encoded":{"operator_epoch":4,"generation":5,"sequence":6,"raw_generation":1,"source_start":2,"source_end":3,"oldest_accepted_us":10,"raw_publication_us":14,"production_device_us":15},"backlog_frames":null})
        );
    }
    let raw = RawProgress {
        generation: 1,
        committed_frames: 3,
        committed_at_us: None,
        ..raw
    };
    let accepted = AcceptedInput {
        at_us: None,
        ..accepted
    };
    let stale = serde_json::to_value(project_media_progress(
        21,
        8,
        stream_runtime::health::State::Healthy,
        Some(&accepted),
        Some(&raw),
        Some(&output),
        Some(0),
    ))
    .unwrap();
    assert!(stale["backlog_frames"].is_null());
    assert_eq!(stale["encoded"]["operator_epoch"], 4);
    let value = serde_json::to_value(project_media_progress(
        21,
        4,
        stream_runtime::health::State::Healthy,
        Some(&accepted),
        Some(&raw),
        Some(&output),
        Some(0),
    ))
    .unwrap();
    assert!(value["accepted"]["observed_device_us"].is_null());
    assert!(value["raw"]["publication_device_us"].is_null());
    assert_eq!(value["backlog_frames"], 0);
}
#[test]
fn latest_departure_replaces_stats_without_inventing_live_health() {
    let cache = Cache::new();
    let mut departure = listener_telemetry::Departure {
        elapsed_us: 1,
        task_runtime_us: 999,
        runtime_valid: false,
        frames_sent: 2,
        bytes_sent: 3,
        frame_write_wall_total_us: 4,
        frame_write_wall_max_us: 5,
        frame_read_wait_wall_total_us: 6,
        payload_elapsed_us: 7,
        block_samples: 8,
        socket: Default::default(),
        media: listener_telemetry::ListenerMedia {
            last_age_us: 50,
            max_age_us: 60,
            queue_frames_max: 70,
            ..Default::default()
        },
        skips: 2,
        frames_skipped: 333,
        last_skip_us: 9,
    };
    cache.publish_last_listener_departure(project_departure(10, 0, Some(&departure)));
    let value = serde_json::to_value(cache.snapshot().unwrap().last_listener_departure).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"observed_device_us":10,"listener_stack_free_bytes":0,"stats":{"elapsed_us":1,"payload_elapsed_us":7,"frames_sent":2,"bytes_sent":3,"frame_write_wall_total_us":4,"frame_write_wall_max_us":5,"frame_read_wait_wall_total_us":6,"task_runtime_us":null,"runtime_valid":false,"block_samples":8,"skips":2,"frames_skipped":333,"last_skip_us":9,"media":null}})
    );
    departure.runtime_valid = true;
    departure.task_runtime_us = 0;
    departure.media.valid = true;
    cache.publish_last_listener_departure(project_departure(11, 0, Some(&departure)));
    let value = serde_json::to_value(cache.snapshot().unwrap().last_listener_departure).unwrap();
    assert_eq!(value["stats"]["task_runtime_us"], 0);
    assert_eq!(value["stats"]["media"]["last_age_us"], 50);
    assert_eq!(value["stats"]["media"]["max_age_us"], 60);
    assert_eq!(value["stats"]["media"]["queue_frames_max"], 70);
    cache.publish_last_listener_departure(project_departure(12, 0, None));
    let value = serde_json::to_value(cache.snapshot().unwrap().last_listener_departure).unwrap();
    assert_eq!(value["observed_device_us"], 12);
    assert!(value["stats"].is_null());
}

fn maximal_stage() -> StageProgress {
    StageProgress {
        observed_device_us: u64::MAX,
        operator_epoch: u32::MAX,
        health: StageHealth::ResetRequested,
        accepted: Some(AcceptedProgress {
            generation: u64::MAX,
            frames: u64::MAX,
            observed_device_us: Some(u64::MAX),
        }),
        raw: Some(RawProgress {
            generation: u64::MAX,
            committed_frames: u64::MAX,
            publication_device_us: Some(u64::MAX),
        }),
        encoded: Some(EncodedProgress {
            operator_epoch: u32::MAX,
            generation: u64::MAX,
            sequence: u64::MAX,
            raw_generation: u64::MAX,
            source_start: u64::MAX,
            source_end: u64::MAX,
            oldest_accepted_us: u64::MAX,
            raw_publication_us: u64::MAX,
            production_device_us: u64::MAX,
        }),
        backlog_frames: Some(u64::MAX),
    }
}
fn maximal_departure() -> LastListenerDeparture {
    LastListenerDeparture {
        observed_device_us: u64::MAX,
        listener_stack_free_bytes: u32::MAX,
        stats: Some(DepartureStats {
            elapsed_us: u64::MAX,
            payload_elapsed_us: u64::MAX,
            frames_sent: u64::MAX,
            bytes_sent: u64::MAX,
            frame_write_wall_total_us: u64::MAX,
            frame_write_wall_max_us: u64::MAX,
            frame_read_wait_wall_total_us: u64::MAX,
            task_runtime_us: Some(u32::MAX),
            runtime_valid: false,
            block_samples: usize::MAX,
            skips: u64::MAX,
            frames_skipped: u64::MAX,
            last_skip_us: u64::MAX,
            media: Some(ListenerMedia {
                last_age_us: u64::MAX,
                max_age_us: u64::MAX,
                queue_frames_max: u64::MAX,
            }),
        }),
    }
}

#[test]
fn network_metadata_has_only_numeric_allowlist_and_unknown_is_null() {
    let cache = Cache::new();
    for connected in [false, true] {
        let network = NetworkMetadata {
            generation: u32::MAX,
            connected,
            disconnects: u32::MAX,
        };
        let value: serde_json::Value = serde_json::from_slice(
            &prepare_response(
                &cache,
                || 20,
                None,
                None,
                Some(network),
                None,
                || [listener_observation::Observation::default(); 2],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            value["network"],
            serde_json::json!({"generation":u32::MAX,"connected":connected,"disconnects":u32::MAX})
        );
    }
    let value: serde_json::Value = serde_json::from_slice(
        &prepare_response(
            &cache,
            || 20,
            None,
            None,
            None,
            None,
            || [listener_observation::Observation::default(); 2],
        )
        .unwrap(),
    )
    .unwrap();
    assert!(value["network"].is_null());
}
#[test]
fn http_stack_watermark_preserves_missing_zero_and_maximum() {
    let cache = Cache::new();
    for watermark in [None, Some(0), Some(u32::MAX)] {
        let body = prepare_response(
            &cache,
            || 1,
            None,
            None,
            None,
            watermark,
            || [listener_observation::Observation::default(); 2],
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value["http_stack_free_bytes"],
            serde_json::to_value(watermark).unwrap()
        );
    }
}
#[test]
fn response_observes_live_listener_progress_without_logging_owner_publication() {
    let cache = Cache::new();
    cache.publish_capture(maximal_capture());
    let registry = listener_observation::Registry::new();
    let owner = registry.register(1).unwrap();
    owner.begin(10, 0, listener_observation::SkipCounters::default());
    let response = |now| {
        serde_json::from_slice::<serde_json::Value>(
            &prepare_response(
                &cache,
                || now,
                None,
                None,
                None,
                None,
                || registry.sample(|| now),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let first = response(20);
    let second = response(40);
    assert_eq!(first["capture"], second["capture"]);
    assert_eq!(first["listeners"][0]["write_us"], 10);
    assert_eq!(second["listeners"][0]["write_us"], 30);
    assert_eq!(second["listeners"][0]["sample_us"], 40);
    drop(owner);
    let released = response(50);
    assert_eq!(released["listeners"][0]["state"], "idle");
    assert_eq!(released["capture"], first["capture"]);
}
#[test]
fn response_samples_listeners_after_cache_unlock_before_served_clock() {
    let cache = Cache::new();
    let capture = maximal_capture();
    cache.publish_capture(capture);
    let registry = listener_observation::Registry::new();
    let owner = registry.register(1).unwrap();
    owner.begin(10, 0, listener_observation::SkipCounters::default());
    let sampled = std::cell::Cell::new(false);
    let body = prepare_response(
        &cache,
        || {
            assert!(sampled.get());
            21
        },
        None,
        None,
        None,
        None,
        || {
            // This real nonblocking publication fails if the response kept its cache guard.
            let mut later = capture;
            later.read_frames = 0;
            assert!(cache.publish_capture(later));
            let observations = registry.sample(|| 20);
            sampled.set(true);
            observations
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["served_device_us"], 21);
    assert_eq!(value["listeners"][0]["sample_us"], 20);
    assert_eq!(value["capture"]["read_frames"], u32::MAX);
    assert_eq!(cache.snapshot().unwrap().capture.unwrap().read_frames, 0);
}
#[test]
fn listener_slots_carry_skip_counters_while_owned_and_null_when_idle() {
    let cache = Cache::new();
    let registry = listener_observation::Registry::new();
    let owner = registry.register(1).unwrap();
    owner.begin(
        10,
        0,
        listener_observation::SkipCounters {
            skips: 2,
            frames_skipped: 333,
            last_skip_us: 9,
        },
    );
    let response = |now| {
        serde_json::from_slice::<serde_json::Value>(
            &prepare_response(
                &cache,
                || now,
                None,
                None,
                None,
                None,
                || registry.sample(|| now),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let writing = response(20);
    assert_eq!(writing["listeners"][0]["state"], "writing");
    assert_eq!(writing["listeners"][0]["skips"], 2);
    assert_eq!(writing["listeners"][0]["frames_skipped"], 333);
    assert_eq!(writing["listeners"][0]["last_skip_us"], 9);
    for key in ["skips", "frames_skipped", "last_skip_us"] {
        assert!(writing["listeners"][1][key].is_null(), "{key}");
    }
    drop(owner);
    let idle = response(30);
    assert_eq!(idle["listeners"][0]["state"], "idle");
    for key in ["skips", "frames_skipped", "last_skip_us"] {
        assert!(idle["listeners"][0][key].is_null(), "{key}");
    }
}
