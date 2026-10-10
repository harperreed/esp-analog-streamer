// ABOUTME: Snapshots encoder observations into fixed records without waiting for export.
// ABOUTME: Keeps owner baselines and probe progression independent of queue loss.
use crate::listener_telemetry::ErrorCode;
use esp_shazam_core::stream_buffer::OutputFormat;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Mutex, TryLockError,
};
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RawReadMetrics {
    pub calls: u64,
    pub attempts: u64,
    pub successes: u64,
    pub frames: u64,
    pub retries: u64,
    pub empty: u64,
    pub overruns: u64,
    pub wait_total_us: u64,
    pub wait_max_us: u64,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub us: u64,
    pub runtime: u32,
    pub listeners: usize,
    pub stack_free_bytes: u32,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Window {
    pub start_us: u64,
    pub end_us: u64,
    pub epoch: u64,
    pub window: u64,
    pub task_runtime_us: u32,
    pub runtime_valid: bool,
    pub listeners_start: usize,
    pub listeners_end: usize,
    pub frames: u64,
    pub resets: u64,
    pub encode_max_us: u64,
    pub stack_free_bytes: u32,
    pub block_samples: usize,
    pub encode_wall_total_us: u64,
    pub encoded_lock_wait_wall_total_us: u64,
    pub encoded_push_held_wall_total_us: u64,
    pub raw_read: RawReadMetrics,
    #[cfg(feature = "encoder-probe")]
    pub probe: crate::encoder_probe::Report,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    Startup,
    Overrun,
    Switched,
    Stopped,
}
#[cfg(feature = "encoder-probe")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Storage {
    pub raw_payload: (usize, usize),
    pub raw_stamps: (usize, usize),
    pub chunk: (usize, usize, usize),
    pub output_address: usize,
    pub output_len: usize,
    pub output_capacity: usize,
    pub output_required: usize,
    pub mp3: Option<(usize, usize, esp_shazam_mp3::EncoderReceipt)>,
}
#[cfg(feature = "encoder-probe")]
impl Storage {
    pub fn snapshot(
        raw: &esp_shazam_core::stream_ring::StreamRing,
        pump: &crate::stream_pipeline::EncoderPump,
        worker: &crate::stream_runtime::Worker,
    ) -> Self {
        assert!(worker.bytes.capacity() >= worker.epoch.format.max_frame_bytes());
        let (payload, payload_bytes) = raw.payload_range();
        let (stamps, stamp_bytes) = raw.stamps_range();
        Self {
            raw_payload: (payload as usize, payload_bytes),
            raw_stamps: (stamps as usize, stamp_bytes),
            chunk: pump.storage_receipt(),
            output_address: worker.bytes.as_ptr() as usize,
            output_len: worker.bytes.len(),
            output_capacity: worker.bytes.capacity(),
            output_required: worker.epoch.format.max_frame_bytes(),
            mp3: worker.encoder.mp3_storage_receipt(),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Boundary {
    pub prior_epoch: u64,
    pub reason: Reason,
    pub frames: u64,
    pub resets: u64,
    pub raw_overruns: u64,
    pub raw_wait_us: u64,
    pub raw_attempts: u64,
    pub baseline_us: u64,
    pub format: OutputFormat,
    #[cfg(feature = "encoder-probe")]
    pub position: (u64, &'static str, u8),
    #[cfg(feature = "encoder-probe")]
    pub sequence: u64,
    #[cfg(feature = "encoder-probe")]
    pub storage: Option<Storage>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Payload {
    Window(Window),
    Boundary(Boundary),
    Failure(ErrorCode),
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Record {
    pub event_us: u64,
    pub epoch: u64,
    pub window: u64,
    pub payload: Payload,
}
pub(crate) struct Queue {
    storage: Mutex<([Option<Record>; 2], usize, usize)>,
    dropped: AtomicU32,
    dropped_full: AtomicU32,
    dropped_contended: AtomicU32,
    dropped_poisoned: AtomicU32,
    loss_occurred: AtomicBool,
}
impl Queue {
    pub const fn new() -> Self {
        Self {
            storage: Mutex::new(([None; 2], 0, 0)),
            dropped: AtomicU32::new(0),
            dropped_full: AtomicU32::new(0),
            dropped_contended: AtomicU32::new(0),
            dropped_poisoned: AtomicU32::new(0),
            loss_occurred: AtomicBool::new(false),
        }
    }
    /// Initialize ESP's lazily allocated pthread mutex before capture, metrics or stream tasks start.
    /// Allocation and waiting may occur here; owner publication only uses the initialized try_lock.
    pub fn initialize(&self) {
        drop(
            self.storage
                .lock()
                .expect("encoder telemetry startup mutex"),
        );
    }
    pub fn publish(&self, record: Record) -> bool {
        let reason = match self.storage.try_lock() {
            Ok(mut storage) => {
                if storage.2 < 2 {
                    let tail = (storage.1 + storage.2) % 2;
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
        self.invalidate();
        self.dropped.fetch_add(1, Ordering::Relaxed);
        false
    }
    pub fn invalidate(&self) {
        self.loss_occurred.store(true, Ordering::Relaxed);
    }
    pub fn take(&self) -> Option<Record> {
        let mut storage = self.storage.try_lock().ok()?;
        if storage.2 == 0 {
            return None;
        }
        let head = storage.1;
        let record = storage.0[head].take();
        storage.1 = (head + 1) % 2;
        storage.2 -= 1;
        record
    }
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::Relaxed)
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
}
pub(crate) static EVENTS: Queue = Queue::new();
/// Maps actual pump outcomes and its successful reset observation to one owner boundary.
pub(crate) fn boundary_reason(
    step: &crate::stream_pipeline::EncoderStep,
    reset: bool,
) -> Option<Reason> {
    use crate::stream_pipeline::EncoderStep;
    match step {
        EncoderStep::Switched => Some(Reason::Switched),
        EncoderStep::Stopped(_) => Some(Reason::Stopped),
        _ if reset => Some(Reason::Overrun),
        _ => None,
    }
}
pub(crate) struct EncoderMetrics {
    pub since_us: u64,
    runtime_us: u32,
    listeners: usize,
    pub epoch: u64,
    pub window: u64,
    pub frames: u64,
    pub resets: u64,
    pub encode_max_us: u64,
    pub encode_wall_total_us: u64,
    pub encoded_lock_wait_wall_total_us: u64,
    pub encoded_push_held_wall_total_us: u64,
    pub raw_read: RawReadMetrics,
    #[cfg(feature = "encoder-probe")]
    pub probe: crate::encoder_probe::Probe,
}
impl EncoderMetrics {
    pub fn observe_reset(&mut self) {
        self.resets += 1;
    }
    pub fn new(sample: Sample) -> Self {
        Self {
            since_us: sample.us,
            runtime_us: sample.runtime,
            listeners: sample.listeners,
            epoch: 0,
            window: 0,
            frames: 0,
            resets: 0,
            encode_max_us: 0,
            encode_wall_total_us: 0,
            encoded_lock_wait_wall_total_us: 0,
            encoded_push_held_wall_total_us: 0,
            raw_read: RawReadMetrics::default(),
            #[cfg(feature = "encoder-probe")]
            probe: crate::encoder_probe::Probe::default(),
        }
    }
    fn baseline(&mut self, sample: Sample) {
        self.since_us = sample.us;
        self.runtime_us = sample.runtime;
        self.listeners = sample.listeners;
        self.encode_max_us = 0;
        self.encode_wall_total_us = 0;
        self.encoded_lock_wait_wall_total_us = 0;
        self.encoded_push_held_wall_total_us = 0;
        self.raw_read = RawReadMetrics::default();
    }
    pub fn report(&mut self, queue: &Queue, sample: Sample, format: OutputFormat) -> bool {
        #[cfg(feature = "encoder-probe")]
        let probe = self.probe.report(
            sample.us - self.since_us,
            self.raw_read.wait_total_us,
            self.encode_wall_total_us,
            self.encoded_lock_wait_wall_total_us,
            self.encoded_push_held_wall_total_us,
        );
        let Some(window) = self.window.checked_add(1) else {
            queue.invalidate();
            self.baseline(sample);
            return false;
        };
        self.window = window;
        let window = Window {
            start_us: self.since_us,
            end_us: sample.us,
            epoch: self.epoch,
            window,
            task_runtime_us: sample.runtime.wrapping_sub(self.runtime_us),
            runtime_valid: sample.us <= u64::from(u32::MAX),
            listeners_start: self.listeners,
            listeners_end: sample.listeners,
            frames: self.frames,
            resets: self.resets,
            encode_max_us: self.encode_max_us,
            stack_free_bytes: sample.stack_free_bytes,
            block_samples: format.block_frames(),
            encode_wall_total_us: self.encode_wall_total_us,
            encoded_lock_wait_wall_total_us: self.encoded_lock_wait_wall_total_us,
            encoded_push_held_wall_total_us: self.encoded_push_held_wall_total_us,
            raw_read: self.raw_read,
            #[cfg(feature = "encoder-probe")]
            probe,
        };
        // Publication cost belongs to the next window, just as the previous logger did.
        self.baseline(sample);
        queue.publish(Record {
            event_us: sample.us,
            epoch: self.epoch,
            window: self.window,
            payload: Payload::Window(window),
        })
    }
    pub fn boundary(
        &mut self,
        queue: &Queue,
        reason: Reason,
        sample: Sample,
        format: OutputFormat,
        #[cfg(feature = "encoder-probe")] storage: Option<Storage>,
    ) -> bool {
        #[cfg(feature = "encoder-probe")]
        let position = self.probe.position();
        #[cfg(feature = "encoder-probe")]
        if reason != Reason::Startup {
            self.probe.invalidate();
        }
        let mut boundary = Boundary {
            prior_epoch: self.epoch,
            reason,
            frames: self.frames,
            resets: self.resets,
            raw_overruns: self.raw_read.overruns,
            raw_wait_us: self.raw_read.wait_total_us,
            raw_attempts: self.raw_read.attempts,
            baseline_us: self.since_us,
            format,
            #[cfg(feature = "encoder-probe")]
            position,
            #[cfg(feature = "encoder-probe")]
            sequence: position.0,
            #[cfg(feature = "encoder-probe")]
            storage,
        };
        if reason == Reason::Switched {
            match self.epoch.checked_add(1) {
                Some(epoch) => self.epoch = epoch,
                None => {
                    queue.invalidate();
                    self.baseline(sample);
                    return false;
                }
            }
            #[cfg(not(feature = "encoder-probe"))]
            {
                self.frames = 0;
                self.resets = 0;
            }
        }
        if reason != Reason::Stopped
            && (cfg!(feature = "encoder-probe") || reason != Reason::Overrun)
        {
            #[cfg(feature = "encoder-probe")]
            {
                let exhausted = format == OutputFormat::Mp3 && self.probe.position().0 == u64::MAX;
                self.probe.restart(format == OutputFormat::Mp3);
                boundary.sequence = self.probe.position().0;
                if exhausted {
                    queue.invalidate();
                    self.baseline(sample);
                    return false;
                }
            }
            self.baseline(sample);
            boundary.baseline_us = sample.us;
        }
        queue.publish(Record {
            event_us: sample.us,
            epoch: self.epoch,
            window: self.window,
            payload: Payload::Boundary(boundary),
        })
    }
    pub fn failure(&self, queue: &Queue, us: u64, error: &std::io::Error) -> bool {
        queue.publish(Record {
            event_us: us,
            epoch: self.epoch,
            window: self.window,
            payload: Payload::Failure(error.into()),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn record(n: u64) -> Record {
        Record {
            event_us: n,
            epoch: 0,
            window: n,
            payload: Payload::Failure(ErrorCode::from(&std::io::Error::other("terminal"))),
        }
    }
    #[test]
    fn queue_counts_full_contended_poisoned_and_sticky_wrap_without_waiting() {
        let q = Queue::new();
        assert!(q.publish(record(1)));
        assert!(q.publish(record(2)));
        assert!(!q.publish(record(3)));
        assert_eq!(q.take().unwrap().event_us, 1);
        assert_eq!(q.take().unwrap().event_us, 2);
        let guard = q.storage.lock().unwrap();
        assert!(!q.publish(record(4)));
        assert!(q.take().is_none());
        drop(guard);
        assert_eq!(
            (
                q.dropped(),
                q.dropped_full(),
                q.dropped_contended(),
                q.dropped_poisoned()
            ),
            (2, 1, 1, 0)
        );
        let _ = std::panic::catch_unwind(|| {
            let _guard = q.storage.lock().unwrap();
            panic!("poison storage");
        });
        q.dropped.store(u32::MAX, Ordering::Relaxed);
        assert!(!q.publish(record(5)));
        assert_eq!((q.dropped(), q.dropped_poisoned()), (0, 1));
        assert!(q.loss_occurred());
        assert!(q.take().is_none());
    }
    #[cfg(feature = "encoder-probe")]
    #[test]
    fn contended_publication_still_advances_actual_probe_and_clears_spans() {
        let q = Queue::new();
        let guard = q.storage.lock().unwrap();
        let mut owner = EncoderMetrics::new(Sample {
            us: 0,
            runtime: 0,
            listeners: 0,
            stack_free_bytes: 1,
        });
        owner.probe.restart(true);
        for i in 0..24 {
            owner.raw_read.wait_total_us = 20;
            owner.encode_wall_total_us = 30;
            assert_eq!(owner.probe.enabled(), (8..16).contains(&i));
            assert!(!owner.report(
                &q,
                Sample {
                    us: (i + 1) * 10_000_000,
                    runtime: 1,
                    listeners: 0,
                    stack_free_bytes: 2
                },
                OutputFormat::Mp3
            ));
            assert_eq!(
                (owner.raw_read.wait_total_us, owner.encode_wall_total_us),
                (0, 0)
            );
        }
        assert_eq!(owner.probe.position(), (1, "COMPLETE", 1));
        assert_eq!(q.dropped_contended(), 24);
        drop(guard);
    }
}
