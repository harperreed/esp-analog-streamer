// ABOUTME: Copies existing owner observations into a bounded numeric metrics cache.
// ABOUTME: Serializes snapshots after releasing the cache mutex.
use crate::device_metrics_identity::{Hex, Identity};
use esp_shazam_core::stream_buffer::{BufferPreset, OutputFormat};
use std::io::Write;
use std::sync::{Mutex, TryLockError};
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Capture {
    pub capture_begin_us: u64,
    pub capture_end_us: u64,
    pub read_frames: u32,
    pub callbacks: u32,
    pub queue_overflows: u32,
    pub software_dropped_frames: u32,
    pub read_errors: u32,
    pub restarts: u32,
    pub supervisor_restarts: u32,
    pub overflow_restarts: u32,
    pub settling_discarded_frames: u32,
    pub discontinuities: u32,
    pub health_restart_requests: u32,
    pub read_service_delay_estimate_max_us: u32,
    pub raw_work_max_us: u32,
    pub meter_work_max_us: u32,
    pub store_work_max_us: u32,
    pub raw_read_max_us: u32,
    pub internal_free: usize,
    pub internal_largest: usize,
    pub internal_min_sum: usize,
    pub psram_free: usize,
    pub psram_largest: usize,
    pub psram_min_sum: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct EncoderWindow {
    pub window_start_us: u64,
    pub window_end_us: u64,
    pub telemetry_epoch: u64,
    pub telemetry_window: u64,
    pub encoded_frames: u64,
    pub resets: u64,
    pub encode_max_us: u64,
    pub raw_read_frames: u64,
    pub raw_read_overruns: u64,
    pub raw_read_retries: u64,
    pub encode_wall_total_us: u64,
    pub raw_wait_total_us: u64,
    pub raw_wait_max_us: u64,
    pub encoded_lock_wait_wall_total_us: u64,
    pub encoded_push_held_wall_total_us: u64,
    pub block_samples: usize,
    pub listeners_start: usize,
    pub listeners_end: usize,
    pub stack_free_bytes: Option<u32>,
    pub task_runtime_us: Option<u32>,
    pub runtime_valid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BoundaryReason {
    Startup,
    Overrun,
    Switched,
    Stopped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum EncoderObservation {
    Window(EncoderWindow),
    Boundary {
        event_us: u64,
        telemetry_epoch: u64,
        telemetry_window: u64,
        reason: BoundaryReason,
    },
    Failure {
        event_us: u64,
        telemetry_epoch: u64,
        telemetry_window: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Loss {
    pub observed_device_us: u64,
    pub dropped: u32,
    pub loss_occurred: bool,
    pub metrics_stack_free_bytes: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StageHealth {
    Healthy,
    ResetRequested,
    Recovering,
    Terminal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct AcceptedProgress {
    pub generation: u64,
    pub frames: u64,
    pub observed_device_us: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct RawProgress {
    pub generation: u64,
    pub committed_frames: u64,
    pub publication_device_us: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct EncodedProgress {
    pub operator_epoch: u32,
    pub generation: u64,
    pub sequence: u64,
    pub raw_generation: u64,
    pub source_start: u64,
    pub source_end: u64,
    pub oldest_accepted_us: u64,
    pub raw_publication_us: u64,
    pub production_device_us: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct StageProgress {
    pub observed_device_us: u64,
    pub operator_epoch: u32,
    pub health: StageHealth,
    pub accepted: Option<AcceptedProgress>,
    pub raw: Option<RawProgress>,
    pub encoded: Option<EncodedProgress>,
    pub backlog_frames: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ListenerMedia {
    pub last_age_us: u64,
    pub max_age_us: u64,
    pub queue_frames_max: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct DepartureStats {
    pub elapsed_us: u64,
    pub payload_elapsed_us: u64,
    pub frames_sent: u64,
    pub bytes_sent: u64,
    pub frame_write_wall_total_us: u64,
    pub frame_write_wall_max_us: u64,
    pub frame_read_wait_wall_total_us: u64,
    pub task_runtime_us: Option<u32>,
    pub runtime_valid: bool,
    pub block_samples: usize,
    pub media: Option<ListenerMedia>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct LastListenerDeparture {
    pub observed_device_us: u64,
    pub listener_stack_free_bytes: u32,
    pub stats: Option<DepartureStats>,
}
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub(crate) struct Snapshot {
    pub capture: Option<Capture>,
    pub encoder: Option<EncoderObservation>,
    pub listener_loss: Option<Loss>,
    pub encoder_loss: Option<Loss>,
    pub media_progress: Option<StageProgress>,
    pub last_listener_departure: Option<LastListenerDeparture>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unavailable {
    Contended,
    Poisoned,
}
pub(crate) struct Cache {
    storage: Mutex<Snapshot>,
}
pub(crate) static CACHE: Cache = Cache::new();
const _: () = assert!(std::mem::size_of::<Cache>() <= 768);
const _: () = assert!(std::mem::size_of::<Snapshot>() <= 768);
impl Cache {
    pub const fn new() -> Self {
        Self {
            storage: Mutex::new(Snapshot {
                capture: None,
                encoder: None,
                listener_loss: None,
                encoder_loss: None,
                media_progress: None,
                last_listener_departure: None,
            }),
        }
    }
    pub fn initialize(&self) {
        drop(self.storage.lock().expect("metrics startup mutex"));
    }
    pub fn snapshot(&self) -> Result<Snapshot, Unavailable> {
        match self.storage.try_lock() {
            Ok(value) => Ok(*value),
            Err(TryLockError::WouldBlock) => Err(Unavailable::Contended),
            Err(TryLockError::Poisoned(_)) => Err(Unavailable::Poisoned),
        }
    }
    pub fn publish_capture(&self, value: Capture) -> bool {
        match self.storage.try_lock() {
            Ok(mut snapshot) => {
                snapshot.capture = Some(value);
                true
            }
            Err(_) => false,
        }
    }
    pub fn publish_encoder(&self, value: EncoderObservation) -> bool {
        match self.storage.try_lock() {
            Ok(mut snapshot) => {
                snapshot.encoder = Some(value);
                true
            }
            Err(_) => false,
        }
    }
    pub fn publish_listener_loss(&self, value: Loss) -> bool {
        match self.storage.try_lock() {
            Ok(mut snapshot) => {
                snapshot.listener_loss = Some(value);
                true
            }
            Err(_) => false,
        }
    }
    pub fn publish_encoder_loss(&self, value: Loss) -> bool {
        match self.storage.try_lock() {
            Ok(mut snapshot) => {
                snapshot.encoder_loss = Some(value);
                true
            }
            Err(_) => false,
        }
    }
    pub fn publish_media_progress(&self, value: StageProgress) -> bool {
        match self.storage.try_lock() {
            Ok(mut snapshot) => {
                snapshot.media_progress = Some(value);
                true
            }
            Err(_) => false,
        }
    }
    pub fn publish_last_listener_departure(&self, value: LastListenerDeparture) -> bool {
        match self.storage.try_lock() {
            Ok(mut snapshot) => {
                snapshot.last_listener_departure = Some(value);
                true
            }
            Err(_) => false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Settings {
    pub active_format: OutputFormat,
    pub next_play_format: OutputFormat,
    pub next_play_buffer: BufferPreset,
    pub buffer_available: bool,
    pub autoplay: bool,
    pub recognition_enabled: bool,
    pub pga_gain: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct NetworkMetadata {
    pub generation: u32,
    pub connected: bool,
    pub disconnects: u32,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
struct Response {
    schema_version: u8,
    served_device_us: u64,
    boot_token: Option<Hex<16>>,
    build_elf_sha256: Option<Hex<32>>,
    settings: Option<Settings>,
    network: Option<NetworkMetadata>,
    #[serde(flatten)]
    snapshot: Snapshot,
}
const _: () = assert!(std::mem::size_of::<Response>() <= 768);
pub(crate) const MAX_BODY_BYTES: usize = 4096;
#[derive(Debug)]
pub(crate) enum ResponseError {
    Unavailable(Unavailable),
    Allocation,
    Serialization,
}
struct BoundedWriter(Vec<u8>);
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_BODY_BYTES - self.0.len() {
            return Err(std::io::Error::other("metrics body limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn prepare_response(
    cache: &Cache,
    clock: impl FnOnce() -> u64,
    identity: Option<Identity>,
    settings: Option<Settings>,
    network: Option<NetworkMetadata>,
) -> Result<Vec<u8>, ResponseError> {
    let snapshot = cache.snapshot().map_err(ResponseError::Unavailable)?;
    let served_device_us = clock();
    let mut body = Vec::new();
    body.try_reserve_exact(MAX_BODY_BYTES)
        .map_err(|_| ResponseError::Allocation)?;
    let mut writer = BoundedWriter(body);
    serde_json::to_writer(
        &mut writer,
        &Response {
            schema_version: 1,
            served_device_us,
            boot_token: identity.map(|identity| identity.boot_token),
            build_elf_sha256: identity.and_then(|identity| identity.build_elf_sha256),
            settings,
            network,
            snapshot,
        },
    )
    .map_err(|_| ResponseError::Serialization)?;
    Ok(writer.0)
}
pub(crate) fn project_encoder(
    record: &crate::encoder_telemetry::Record,
) -> Option<EncoderObservation> {
    use crate::encoder_telemetry::{Payload, Reason};
    Some(match &record.payload {
        Payload::Window(w) => EncoderObservation::Window(EncoderWindow {
            window_start_us: w.start_us,
            window_end_us: w.end_us,
            telemetry_epoch: w.epoch,
            telemetry_window: w.window,
            encoded_frames: w.frames,
            resets: w.resets,
            encode_max_us: w.encode_max_us,
            raw_read_frames: w.raw_read.frames,
            raw_read_overruns: w.raw_read.overruns,
            raw_read_retries: w.raw_read.retries,
            encode_wall_total_us: w.encode_wall_total_us,
            raw_wait_total_us: w.raw_read.wait_total_us,
            raw_wait_max_us: w.raw_read.wait_max_us,
            encoded_lock_wait_wall_total_us: w.encoded_lock_wait_wall_total_us,
            encoded_push_held_wall_total_us: w.encoded_push_held_wall_total_us,
            block_samples: w.block_samples,
            listeners_start: w.listeners_start,
            listeners_end: w.listeners_end,
            stack_free_bytes: Some(w.stack_free_bytes),
            task_runtime_us: w.runtime_valid.then_some(w.task_runtime_us),
            runtime_valid: w.runtime_valid,
        }),
        Payload::Boundary(b) => EncoderObservation::Boundary {
            event_us: record.event_us,
            telemetry_epoch: record.epoch,
            telemetry_window: record.window,
            reason: match b.reason {
                Reason::Startup => BoundaryReason::Startup,
                Reason::Overrun => BoundaryReason::Overrun,
                Reason::Switched => BoundaryReason::Switched,
                Reason::Stopped => BoundaryReason::Stopped,
            },
        },
        Payload::Failure(_) => EncoderObservation::Failure {
            event_us: record.event_us,
            telemetry_epoch: record.epoch,
            telemetry_window: record.window,
        },
        Payload::Health(_) => return None,
    })
}
pub(crate) fn project_media_progress(
    observed_device_us: u64,
    operator_epoch: u32,
    health: crate::stream_runtime::health::State,
    accepted: Option<&esp_shazam_core::stream_ring::AcceptedInput>,
    raw: Option<&esp_shazam_core::stream_ring::RawProgress>,
    output: Option<&crate::stream_runtime::health::OutputProgress>,
    backlog_frames: Option<u64>,
) -> StageProgress {
    use crate::stream_runtime::health::State;
    StageProgress {
        observed_device_us,
        operator_epoch,
        health: match health {
            State::Healthy => StageHealth::Healthy,
            State::ResetRequested => StageHealth::ResetRequested,
            State::Recovering => StageHealth::Recovering,
            State::Terminal => StageHealth::Terminal,
        },
        accepted: accepted.map(|value| AcceptedProgress {
            generation: value.generation,
            frames: value.frames,
            observed_device_us: value.at_us,
        }),
        raw: raw.map(|value| RawProgress {
            generation: value.generation,
            committed_frames: value.committed_frames,
            publication_device_us: value.committed_at_us,
        }),
        encoded: output.map(|value| {
            let source = &value.receipt.metadata.source;
            EncodedProgress {
                operator_epoch: value.operator_epoch,
                generation: value.receipt.generation,
                sequence: value.receipt.sequence,
                raw_generation: source.raw_generation,
                source_start: source.source_start,
                source_end: source.source_end,
                oldest_accepted_us: source.oldest_accepted_us,
                raw_publication_us: source.raw_publication_us,
                production_device_us: value.receipt.metadata.encoded_production_us,
            }
        }),
        backlog_frames: backlog_frames
            .filter(|_| output.is_some_and(|value| value.operator_epoch == operator_epoch)),
    }
}
pub(crate) fn project_departure(
    observed_device_us: u64,
    listener_stack_free_bytes: u32,
    metrics: Option<&crate::listener_telemetry::Departure>,
) -> LastListenerDeparture {
    LastListenerDeparture {
        observed_device_us,
        listener_stack_free_bytes,
        stats: metrics.map(|value| DepartureStats {
            elapsed_us: value.elapsed_us,
            payload_elapsed_us: value.payload_elapsed_us,
            frames_sent: value.frames_sent,
            bytes_sent: value.bytes_sent,
            frame_write_wall_total_us: value.frame_write_wall_total_us,
            frame_write_wall_max_us: value.frame_write_wall_max_us,
            frame_read_wait_wall_total_us: value.frame_read_wait_wall_total_us,
            task_runtime_us: value.runtime_valid.then_some(value.task_runtime_us),
            runtime_valid: value.runtime_valid,
            block_samples: value.block_samples,
            media: value.media.valid.then_some(ListenerMedia {
                last_age_us: value.media.last_age_us,
                max_age_us: value.media.max_age_us,
                queue_frames_max: value.media.queue_frames_max,
            }),
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc};
    use std::time::Duration;
    fn loss() -> Loss {
        Loss {
            observed_device_us: 1,
            dropped: 0,
            loss_occurred: false,
            metrics_stack_free_bytes: None,
        }
    }
    #[test]
    fn response_fits_with_network_metadata() {
        assert!(std::mem::size_of::<Response>() <= 768);
    }
    #[test]
    fn writer_rejects_4097_bytes() {
        let mut writer = BoundedWriter(Vec::with_capacity(MAX_BODY_BYTES));
        assert!(writer.write_all(&vec![0; MAX_BODY_BYTES + 1]).is_err());
        assert!(writer.0.is_empty());
    }
    #[test]
    fn contention_and_poison_do_not_recover() {
        let cache = Arc::new(Cache::new());
        let guard = cache.storage.lock().unwrap();
        let (tx, rx) = mpsc::channel();
        let other = cache.clone();
        let thread = std::thread::spawn(move || {
            assert!(!other.publish_capture(Capture {
                capture_begin_us: 0,
                capture_end_us: 0,
                read_frames: 0,
                callbacks: 0,
                queue_overflows: 0,
                software_dropped_frames: 0,
                read_errors: 0,
                restarts: 0,
                supervisor_restarts: 0,
                overflow_restarts: 0,
                settling_discarded_frames: 0,
                discontinuities: 0,
                health_restart_requests: 0,
                read_service_delay_estimate_max_us: 0,
                raw_work_max_us: 0,
                meter_work_max_us: 0,
                store_work_max_us: 0,
                raw_read_max_us: 0,
                internal_free: 0,
                internal_largest: 0,
                internal_min_sum: 0,
                psram_free: 0,
                psram_largest: 0,
                psram_min_sum: 0
            }));
            assert!(!other.publish_encoder(EncoderObservation::Failure {
                event_us: 0,
                telemetry_epoch: 0,
                telemetry_window: 0
            }));
            assert!(!other.publish_media_progress(project_media_progress(
                1,
                1,
                crate::stream_runtime::health::State::Healthy,
                None,
                None,
                None,
                None
            )));
            assert!(!other.publish_last_listener_departure(project_departure(1, 0, None)));
            assert!(!other.publish_listener_loss(loss()));
            assert!(!other.publish_encoder_loss(loss()));
            assert_eq!(other.snapshot().unwrap_err(), Unavailable::Contended);
            assert!(matches!(
                prepare_response(&other, || 1, None, None, None),
                Err(ResponseError::Unavailable(Unavailable::Contended))
            ));
            tx.send(()).unwrap();
        });
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        thread.join().unwrap();
        drop(guard);
        let unchanged = cache.snapshot().unwrap();
        assert!(unchanged.listener_loss.is_none());
        assert!(unchanged.media_progress.is_none());
        assert!(unchanged.last_listener_departure.is_none());
        let other = cache.clone();
        assert!(std::thread::spawn(move || {
            let _guard = other.storage.lock().unwrap();
            panic!("poison fixture");
        })
        .join()
        .is_err());
        assert!(!cache.publish_capture(Capture {
            capture_begin_us: 0,
            capture_end_us: 0,
            read_frames: 0,
            callbacks: 0,
            queue_overflows: 0,
            software_dropped_frames: 0,
            read_errors: 0,
            restarts: 0,
            supervisor_restarts: 0,
            overflow_restarts: 0,
            settling_discarded_frames: 0,
            discontinuities: 0,
            health_restart_requests: 0,
            read_service_delay_estimate_max_us: 0,
            raw_work_max_us: 0,
            meter_work_max_us: 0,
            store_work_max_us: 0,
            raw_read_max_us: 0,
            internal_free: 0,
            internal_largest: 0,
            internal_min_sum: 0,
            psram_free: 0,
            psram_largest: 0,
            psram_min_sum: 0
        }));
        assert!(!cache.publish_encoder(EncoderObservation::Failure {
            event_us: 0,
            telemetry_epoch: 0,
            telemetry_window: 0
        }));
        assert!(!cache.publish_media_progress(project_media_progress(
            1,
            1,
            crate::stream_runtime::health::State::Healthy,
            None,
            None,
            None,
            None
        )));
        assert!(!cache.publish_last_listener_departure(project_departure(1, 0, None)));
        assert!(!cache.publish_listener_loss(loss()));
        assert!(!cache.publish_encoder_loss(loss()));
        assert_eq!(cache.snapshot().unwrap_err(), Unavailable::Poisoned);
    }
    #[test]
    fn serialization_does_not_hold_cache_guard() {
        struct PausedWriter {
            entered: mpsc::Sender<()>,
            resume: mpsc::Receiver<()>,
            paused: bool,
        }
        impl Write for PausedWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if !self.paused {
                    self.paused = true;
                    self.entered.send(()).unwrap();
                    self.resume.recv_timeout(Duration::from_secs(5)).unwrap();
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let cache = Cache::new();
        let snapshot = cache.snapshot().unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let serializer = std::thread::spawn(move || {
            serde_json::to_writer(
                PausedWriter {
                    entered: entered_tx,
                    resume: resume_rx,
                    paused: false,
                },
                &snapshot,
            )
            .unwrap()
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(cache.publish_listener_loss(loss()));
        resume_tx.send(()).unwrap();
        serializer.join().unwrap();
    }
    #[test]
    fn unavailable_crosses_real_sockets_and_contention_recovers() {
        use std::io::Read;
        use std::net::{TcpListener, TcpStream};
        fn read(cache: Arc<Cache>) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0; 128];
                assert!(socket.read(&mut request).unwrap() > 0);
                let prepared = prepare_response(&cache, || 1, None, None, None);
                let (status, body) = match prepared {
                    Ok(body) => (200, body),
                    Err(_) => (503, b"{\"error\":\"metrics_unavailable\"}".to_vec()),
                };
                write!(
                    socket,
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .unwrap();
                socket.write_all(&body).unwrap();
            });
            let mut socket = TcpStream::connect(address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .write_all(b"GET /api/metrics HTTP/1.1\r\n\r\n")
                .unwrap();
            let mut response = String::new();
            socket.read_to_string(&mut response).unwrap();
            server.join().unwrap();
            response
        }
        let cache = Arc::new(Cache::new());
        let guard = cache.storage.lock().unwrap();
        let response = read(cache.clone());
        assert!(response.starts_with("HTTP/1.1 503"));
        assert!(response.ends_with("{\"error\":\"metrics_unavailable\"}"));
        drop(guard);
        assert!(read(cache.clone()).starts_with("HTTP/1.1 200"));
        let other = cache.clone();
        assert!(std::thread::spawn(move || {
            let _guard = other.storage.lock().unwrap();
            panic!("poison fixture");
        })
        .join()
        .is_err());
        assert!(read(cache).starts_with("HTTP/1.1 503"));
    }
    #[test]
    fn contended_new_families_keep_previous_observations() {
        let cache = Arc::new(Cache::new());
        let stage = project_media_progress(
            1,
            2,
            crate::stream_runtime::health::State::Healthy,
            None,
            None,
            None,
            Some(0),
        );
        let departure = project_departure(3, 0, None);
        assert!(cache.publish_media_progress(stage));
        assert!(cache.publish_last_listener_departure(departure));
        let guard = cache.storage.lock().unwrap();
        let other = cache.clone();
        let (tx, rx) = mpsc::channel();
        let writer = std::thread::spawn(move || {
            assert!(!other.publish_media_progress(StageProgress {
                observed_device_us: 9,
                ..stage
            }));
            assert!(
                !other.publish_last_listener_departure(LastListenerDeparture {
                    observed_device_us: 10,
                    ..departure
                })
            );
            tx.send(()).unwrap();
        });
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        writer.join().unwrap();
        drop(guard);
        let snapshot = cache.snapshot().unwrap();
        assert_eq!(snapshot.media_progress, Some(stage));
        assert_eq!(snapshot.last_listener_departure, Some(departure));
    }
    #[test]
    fn concurrent_families_remain_coherent() {
        let cache = Arc::new(Cache::new());
        let other = cache.clone();
        let writer = std::thread::spawn(move || {
            for n in 0..10000 {
                other.publish_listener_loss(Loss {
                    observed_device_us: n,
                    dropped: n as u32,
                    ..loss()
                });
                let accepted = esp_shazam_core::stream_ring::AcceptedInput {
                    generation: n,
                    frames: n,
                    at_us: Some(n),
                };
                other.publish_media_progress(project_media_progress(
                    n,
                    n as u32,
                    crate::stream_runtime::health::State::Healthy,
                    Some(&accepted),
                    None,
                    None,
                    None,
                ));
                other.publish_last_listener_departure(project_departure(n, n as u32, None));
            }
        });
        for _ in 0..10000 {
            if let Ok(snapshot) = cache.snapshot() {
                if let Some(value) = snapshot.listener_loss {
                    assert_eq!(value.observed_device_us, u64::from(value.dropped));
                }
                if let Some(value) = snapshot.media_progress {
                    let accepted = value.accepted.unwrap();
                    assert_eq!(value.observed_device_us, accepted.generation);
                    assert_eq!(accepted.generation, accepted.frames);
                    assert_eq!(accepted.observed_device_us, Some(accepted.frames));
                }
                if let Some(value) = snapshot.last_listener_departure {
                    assert_eq!(
                        value.observed_device_us,
                        u64::from(value.listener_stack_free_bytes)
                    );
                }
            }
        }
        writer.join().unwrap();
    }
}
