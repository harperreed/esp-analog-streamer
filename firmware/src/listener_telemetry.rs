// ABOUTME: Stores bounded listener events without waiting for diagnostic output.
// ABOUTME: Keeps socket cleanup and admission release independent of the exporter.
use crate::stream_pipeline::{DrainingSocket, RetainedClose};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Mutex, TryLockError,
    },
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub(crate) struct Record {
    pub event_us: u64,
    pub peer: Option<SocketAddr>,
    pub payload: Payload,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ListenerMedia {
    pub valid: bool,
    pub first_raw_generation: u64,
    pub last_raw_generation: u64,
    pub source_start: u64,
    pub source_end: u64,
    pub first_encoded_generation: u64,
    pub last_encoded_generation: u64,
    pub first_encoded_sequence: u64,
    pub last_encoded_sequence: u64,
    pub last_age_us: u64,
    pub max_age_us: u64,
    pub queue_frames_max: u64,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Departure {
    pub elapsed_us: u64,
    pub task_runtime_us: u32,
    pub runtime_valid: bool,
    pub frames_sent: u64,
    pub bytes_sent: u64,
    pub frame_write_wall_total_us: u64,
    pub frame_write_wall_max_us: u64,
    pub frame_read_wait_wall_total_us: u64,
    pub payload_elapsed_us: u64,
    pub block_samples: usize,
    pub socket: esp_shazam_core::stream_http::WriterDiagnostics,
    pub media: ListenerMedia,
    /// Live-policy resynchronizations: events, whole frames passed over, the last event's time.
    pub skips: u64,
    pub frames_skipped: u64,
    pub last_skip_us: u64,
}
/// The departure receipt line the harnesses parse (`scripts/network_fault.py` `LEFT`). The skip
/// tokens sit between the end text and the stack watermark; a departure without listener
/// metrics (the WAV path) carries none, which the parsers read as unreported.
pub(crate) struct DepartureLine<'a, E> {
    pub peer: &'a str,
    pub tenths: u64,
    pub end: E,
    pub metrics: Option<&'a Departure>,
    pub owner_stack_free_bytes: u32,
    pub event_us: u64,
}
impl<E: std::fmt::Display> std::fmt::Display for DepartureLine<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "stream: {} left after {}.{} s: {}",
            self.peer,
            self.tenths / 10,
            self.tenths % 10,
            self.end
        )?;
        if let Some(m) = self.metrics {
            write!(
                f,
                " skips={} frames_skipped={} last_skip_us={}",
                m.skips, m.frames_skipped, m.last_skip_us
            )?;
        }
        write!(
            f,
            " listener_stack_free_bytes={} event_us={}",
            self.owner_stack_free_bytes, self.event_us
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum EndCode {
    Retired,
    NetworkChanged,
    FellBehind,
    StoppedReading,
    CaptureStalled,
    Failed(ErrorCode),
    InvalidFrame(ErrorCode),
    EncodedReset,
    InvalidCursor,
    MediaAge,
    EncoderUnhealthy,
}
impl From<&crate::stream_pipeline::End> for EndCode {
    fn from(end: &crate::stream_pipeline::End) -> Self {
        use crate::stream_pipeline::End;
        match end {
            End::Retired => Self::Retired,
            End::NetworkChanged => Self::NetworkChanged,
            End::EncodedReset => Self::EncodedReset,
            End::InvalidCursor => Self::InvalidCursor,
            End::MediaAge => Self::MediaAge,
            End::EncoderUnhealthy => Self::EncoderUnhealthy,
            End::FellBehind => Self::FellBehind,
            End::StoppedReading => Self::StoppedReading,
            End::CaptureStalled => Self::CaptureStalled,
            End::Failed(error) => Self::Failed(error.into()),
            End::InvalidFrame(error) => Self::InvalidFrame(error.into()),
        }
    }
}
// The fixed queue reserves its largest record at setup. Boxing departures would
// allocate on the nonblocking producer path and undermine bounded diagnostics.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Payload {
    Joined {
        format: esp_shazam_core::stream_http::Format,
        held: usize,
        owner_id: u64,
        sonos_intent: u64,
    },
    Departure {
        metrics: Option<Departure>,
        tenths: u64,
        end: EndCode,
        owner_stack_free_bytes: u32,
    },
    Closed {
        elapsed_ms: u64,
        held: usize,
        owner_id: u64,
        sonos_intent: u64,
        released_us: u64,
        /// The reaper task's lifetime stack minimum, sampled after this close.
        reaper_stack_free_bytes: u32,
    },
    Timeout {
        elapsed_ms: u64,
        nominal_limit_ms: u64,
        remaining_budget_us: u64,
        deadline_us: Option<u64>,
        bytes: usize,
    },
    Refusal {
        reason: RefusalReason,
        status: u16,
        response_error: Option<ErrorCode>,
    },
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum RefusalReason {
    BufferedStartup,
    BufferAllocation(ErrorCode),
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct ErrorCode {
    pub kind: std::io::ErrorKind,
    pub raw_os_error: Option<i32>,
}
/// Captures the budget at its timing origin and a boot deadline when known.
#[derive(Clone, Copy)]
pub(crate) struct WriteTiming {
    pub nominal_limit_ms: u64,
    pub remaining_budget_us: u64,
    pub deadline_us: Option<u64>,
}
impl WriteTiming {
    pub(crate) fn new(started_us: u64, deadline_us: u64, nominal_limit_ms: u64) -> Self {
        Self {
            nominal_limit_ms,
            remaining_budget_us: deadline_us.saturating_sub(started_us),
            deadline_us: Some(deadline_us),
        }
    }
    /// The Instant write starts with this full budget; its boot-clock deadline is unknown.
    pub(crate) fn for_instant_limit(limit: Duration) -> Self {
        Self {
            nominal_limit_ms: limit.as_millis() as u64,
            remaining_budget_us: limit.as_micros() as u64,
            deadline_us: None,
        }
    }
}

pub(crate) fn report_timeout(
    queue: &Queue,
    peer: Option<SocketAddr>,
    result: &std::io::Result<()>,
    elapsed_ms: u64,
    timing: WriteTiming,
    bytes: usize,
    now: impl FnOnce() -> u64,
) {
    if matches!(result, Err(error) if matches!(error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock))
    {
        queue.publish(Record {
            event_us: now(),
            peer,
            payload: Payload::Timeout {
                elapsed_ms,
                nominal_limit_ms: timing.nominal_limit_ms,
                remaining_budget_us: timing.remaining_budget_us,
                deadline_us: timing.deadline_us,
                bytes,
            },
        });
    }
}
pub(crate) fn send_admitted_refusal(
    socket: &std::net::TcpStream,
    queue: &Queue,
    peer: Option<SocketAddr>,
    reason: RefusalReason,
    timeout: Duration,
    response: &str,
    now: impl FnOnce() -> u64,
) {
    let sent = esp_shazam_core::stream_http::write_response(socket, response.as_bytes(), timeout);
    queue.publish(Record {
        event_us: now(),
        peer,
        payload: Payload::Refusal {
            reason,
            status: 503,
            response_error: sent.as_ref().err().map(ErrorCode::from),
        },
    });
}
impl From<&std::io::Error> for ErrorCode {
    fn from(error: &std::io::Error) -> Self {
        Self {
            kind: error.kind(),
            raw_os_error: error.raw_os_error(),
        }
    }
}
pub(crate) struct Queue {
    storage: Mutex<([Option<Record>; 8], usize, usize)>,
    dropped: AtomicU32,
    dropped_full: AtomicU32,
    dropped_contended: AtomicU32,
    dropped_poisoned: AtomicU32,
    loss_occurred: AtomicBool,
}
impl Queue {
    /// Initialize the target's lazy pthread allocation before producer/exporter threads start.
    /// Only setup calls this blocking operation; publication remains try-lock-only.
    pub fn initialize(&self) {
        drop(self.storage.lock().expect("listener telemetry setup mutex"));
    }

    pub const fn new() -> Self {
        Self {
            storage: Mutex::new(([None; 8], 0, 0)),
            dropped: AtomicU32::new(0),
            dropped_full: AtomicU32::new(0),
            dropped_contended: AtomicU32::new(0),
            dropped_poisoned: AtomicU32::new(0),
            loss_occurred: AtomicBool::new(false),
        }
    }
    pub fn publish(&self, record: Record) -> bool {
        let reason = match self.storage.try_lock() {
            Ok(mut storage) => {
                if storage.2 < 8 {
                    let tail = (storage.1 + storage.2) % 8;
                    storage.0[tail] = Some(record);
                    storage.2 += 1;
                    return true;
                }
                &self.dropped_full
            }
            Err(TryLockError::WouldBlock) => &self.dropped_contended,
            Err(TryLockError::Poisoned(_)) => &self.dropped_poisoned,
        };
        reason.fetch_add(1, Ordering::Relaxed);
        self.loss_occurred.store(true, Ordering::Relaxed);
        self.dropped.fetch_add(1, Ordering::Relaxed);
        false
    }
    pub fn take(&self) -> Option<Record> {
        let mut storage = self.storage.try_lock().ok()?;
        if storage.2 == 0 {
            return None;
        }
        let head = storage.1;
        let record = storage.0[head].take();
        storage.1 = (head + 1) % 8;
        storage.2 -= 1;
        record
    }
    pub fn dropped_full(&self) -> u32 {
        self.dropped_full.load(Ordering::Relaxed)
    }
    pub fn dropped_contended(&self) -> u32 {
        self.dropped_contended.load(Ordering::Relaxed)
    }
    pub fn dropped_poisoned(&self) -> u32 {
        self.dropped_poisoned.load(Ordering::Relaxed)
    }
    pub fn loss_occurred(&self) -> bool {
        self.loss_occurred.load(Ordering::Relaxed)
    }
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::Relaxed)
    }
}
/// Runs the checked close of a socket whose slot was released at hand-off, then stamps the
/// Closed receipt with the closing task's stack minimum. A failure publishes nothing and
/// returns the retained socket.
pub(crate) fn close_and_report<S, F>(
    draining: DrainingSocket<S>,
    queue: &Queue,
    peer: Option<SocketAddr>,
    now: impl FnOnce() -> u64,
    stack_free: impl FnOnce() -> u32,
    close: impl FnOnce(S) -> Result<Duration, F>,
) -> Result<(), RetainedClose<F>> {
    let DrainingSocket {
        owner_id,
        sonos_intent,
        held,
        released_us,
        ..
    } = draining;
    let elapsed = draining.close(close)?;
    let reaper_stack_free_bytes = stack_free();
    queue.publish(Record {
        event_us: now(),
        peer,
        payload: Payload::Closed {
            elapsed_ms: elapsed.as_millis() as u64,
            held,
            owner_id,
            sonos_intent,
            released_us,
            reaper_stack_free_bytes,
        },
    });
    Ok(())
}

pub(crate) static EVENTS: Queue = Queue::new();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_pipeline::ConnectionResources;
    #[test]
    fn slot_release_precedes_close_timestamp() {
        use esp_shazam_core::stream_wait::{Capacity, Slot};
        use std::{
            io::Read,
            net::{Shutdown, TcpListener, TcpStream},
            sync::Arc,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let (socket, address) = listener.accept().unwrap();
        let capacity = Arc::new(Capacity::default());
        let slot = Slot::take(&capacity, 2).unwrap();
        let expected_owner = slot.owner_id();
        let queue = Queue::new();
        let draining = ConnectionResources::new(socket, slot, None).release(|| 100);
        assert_eq!(capacity.held(), 0, "the slot is free before the close");
        assert!(close_and_report(
            draining,
            &queue,
            Some(address),
            || {
                assert_eq!(capacity.held(), 0, "close never holds the slot");
                123
            },
            || 777,
            |socket| {
                socket.shutdown(Shutdown::Both)?;
                drop(socket);
                Ok::<_, std::io::Error>(Duration::from_millis(2))
            }
        )
        .is_ok());
        assert_eq!(peer.read(&mut [0]).unwrap(), 0);
        let record = queue.take().unwrap();
        assert_eq!(record.event_us, 123);
        assert_eq!(record.peer, Some(address));
        assert!(matches!(record.payload, Payload::Closed {
            owner_id, sonos_intent: 0, elapsed_ms: 2, held: 1, released_us: 100,
            reaper_stack_free_bytes: 777,
        } if owner_id == expected_owner));
    }
    fn record() -> Record {
        Record {
            event_us: 12,
            peer: None,
            payload: Payload::Closed {
                elapsed_ms: 3,
                held: 1,
                owner_id: 1,
                sonos_intent: 0,
                released_us: 10,
                reaper_stack_free_bytes: 0,
            },
        }
    }
    #[test]
    fn publication_counts_exact_reasons_and_wraps_with_sticky_loss() {
        let q = Queue::new();
        for _ in 0..8 {
            assert!(q.publish(record()));
        }
        assert_eq!(
            (
                q.dropped(),
                q.dropped_full(),
                q.dropped_contended(),
                q.dropped_poisoned()
            ),
            (0, 0, 0, 0)
        );
        assert!(!q.loss_occurred());
        assert!(!q.publish(record()));
        assert_eq!(
            (
                q.dropped(),
                q.dropped_full(),
                q.dropped_contended(),
                q.dropped_poisoned()
            ),
            (1, 1, 0, 0)
        );
        for _ in 0..8 {
            assert!(q.take().is_some());
        }
        assert!(q.publish(record()));
        assert_eq!(q.dropped_full(), 1);
        let guard = q.storage.lock().unwrap();
        q.dropped.store(u32::MAX, Ordering::Relaxed);
        q.dropped_contended.store(u32::MAX, Ordering::Relaxed);
        assert!(!q.publish(record()));
        assert_eq!(
            (
                q.dropped(),
                q.dropped_full(),
                q.dropped_contended(),
                q.dropped_poisoned()
            ),
            (0, 1, 0, 0)
        );
        assert!(q.loss_occurred());
        drop(guard);
        let _ = std::panic::catch_unwind(|| {
            let _guard = q.storage.lock().unwrap();
            panic!("poison storage");
        });
        q.dropped_poisoned.store(u32::MAX, Ordering::Relaxed);
        assert!(!q.publish(record()));
        assert_eq!(
            (
                q.dropped(),
                q.dropped_full(),
                q.dropped_contended(),
                q.dropped_poisoned()
            ),
            (1, 1, 0, 0)
        );
        assert!(q.loss_occurred());
        assert!(!q.publish(record()));
        assert_eq!(q.dropped_poisoned(), 1);
        let full = Queue::new();
        for _ in 0..8 {
            assert!(full.publish(record()));
        }
        full.dropped_full.store(u32::MAX, Ordering::Relaxed);
        assert!(!full.publish(record()));
        assert_eq!(full.dropped_full(), 0);
        assert!(full.loss_occurred());
    }
    #[test]
    fn held_and_poisoned_storage_drop_without_waiting_and_loss_wraps() {
        let q = Queue::new();
        let guard = q.storage.lock().unwrap();
        q.dropped.store(u32::MAX, Ordering::Relaxed);
        assert!(!q.publish(record()));
        assert_eq!(q.dropped(), 0);
        assert!(q.loss_occurred());
        assert!(q.take().is_none());
        drop(guard);
        assert!(q.publish(record()));
        let result = std::panic::catch_unwind(|| {
            let _guard = q.storage.lock().unwrap();
            panic!("poison storage");
        });
        assert!(result.is_err());
        assert!(!q.publish(record()));
        assert_eq!(q.dropped(), 1);
        assert!(q.take().is_none());
    }
    #[test]
    fn held_and_poisoned_export_storage_cannot_hold_real_admission_after_close() {
        use esp_shazam_core::stream_wait::{Capacity, Slot};
        use std::{
            io::Read,
            net::{Shutdown, TcpListener, TcpStream},
            sync::Arc,
        };
        for poison in [false, true] {
            let q = Queue::new();
            if poison {
                let _ = std::panic::catch_unwind(|| {
                    let _guard = q.storage.lock().unwrap();
                    panic!("poison storage");
                });
            }
            let _guard = if poison {
                None
            } else {
                Some(q.storage.lock().unwrap())
            };
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
            let (socket, address) = listener.accept().unwrap();
            let slots = Arc::new(Capacity::default());
            let (runtime, _worker) = crate::stream_runtime::Runtime::new(
                esp_shazam_core::stream_buffer::OutputFormat::Flac,
                false,
            )
            .unwrap();
            let (_, lease) = runtime.admit_line("GET /stream.flac HTTP/1.1");
            let resources = ConnectionResources::new(socket, Slot::take(&slots, 2).unwrap(), lease);
            assert!(close_and_report(
                resources.release(|| 11),
                &q,
                Some(address),
                || 12,
                || 0,
                |socket| {
                    socket.shutdown(Shutdown::Both)?;
                    drop(socket);
                    Ok::<_, std::io::Error>(Duration::ZERO)
                }
            )
            .is_ok());
            assert_eq!(slots.held(), 0);
            assert_eq!(peer.read(&mut [0]).unwrap(), 0);
            assert_eq!(q.dropped(), 1);
        }
    }
}
