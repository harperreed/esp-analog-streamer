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
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
fn get(cache: Arc<Cache>, served: u64) -> (u16, serde_json::Value) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 512];
        let n = socket.read(&mut request).unwrap();
        assert!(n > 0);
        let prepared = prepare_response(
            &cache,
            || served,
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
        );
        let (status, body) = match prepared {
            Ok(body) => (200, body),
            Err(_) => (503, b"{\"error\":\"metrics_unavailable\"}".to_vec()),
        };
        write!(socket,"HTTP/1.1 {status} OK\r\nContent-Type: application/json; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
        socket.write_all(&body).unwrap();
    });
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    socket
        .write_all(b"GET /api/metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    server.join().unwrap();
    let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = std::str::from_utf8(&response[..split]).unwrap();
    assert!(head.contains("Cache-Control: no-store"));
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = &response[split + 4..];
    assert!(body.len() <= MAX_BODY_BYTES);
    (status, serde_json::from_slice(body).unwrap())
}
#[test]
fn real_socket_startup_and_freshness() {
    let cache = Arc::new(Cache::new());
    let (status, startup) = get(cache.clone(), 1);
    assert_eq!(status, 200);
    assert!(startup["capture"].is_null());
    cache.publish_listener_loss(Loss {
        observed_device_us: 7,
        dropped: u32::MAX,
        loss_occurred: true,
        metrics_stack_free_bytes: Some(0),
    });
    for served in [10, 20] {
        let (status, value) = get(cache.clone(), served);
        assert_eq!(status, 200);
        assert_eq!(value["served_device_us"], served);
        assert_eq!(value["listener_loss"]["observed_device_us"], 7);
    }
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
fn real_socket_maximal_body() {
    let cache = Arc::new(Cache::new());
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
    let (status, value) = get(cache, u64::MAX);
    assert_eq!(status, 200);
    assert_eq!(value["capture"]["read_frames"], u32::MAX);
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
            media: Some(ListenerMedia {
                last_age_us: u64::MAX,
                max_age_us: u64::MAX,
                queue_frames_max: u64::MAX,
            }),
        }),
    }
}
