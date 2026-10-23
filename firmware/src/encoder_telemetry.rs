// ABOUTME: Snapshots encoder observations into fixed records without waiting for export.
// ABOUTME: Keeps owner baselines and probe progression independent of queue loss.
use crate::listener_telemetry::ErrorCode;
use esp_shazam_core::stream_buffer::OutputFormat;
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
pub(crate) struct HealthTransition {
    pub transition: crate::stream_runtime::health::State,
    pub operator_epoch: u32,
    pub raw_valid: bool,
    pub encoded_valid: bool,
    pub raw_generation: u64,
    pub raw_committed: u64,
    pub encoded_generation: u64,
    pub encoded_sequence: u64,
    pub source_end: u64,
    pub encoded_production_us: u64,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Payload {
    Window(Window),
    Boundary(Boundary),
    Failure(ErrorCode),
    Health(HealthTransition),
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Record {
    pub event_us: u64,
    pub epoch: u64,
    pub window: u64,
    pub payload: Payload,
}
/// Encoder receipts: windows, owner boundaries, failures and health transitions. Two deep: the
/// metrics owner drains two per iteration and the producer publishes at most one per window.
pub(crate) type Queue = crate::listener_telemetry::EventQueue<Record, 2>;
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
#[cfg(all(test, feature = "encoder-probe"))]
mod tests {
    use super::*;
    #[test]
    fn contended_publication_still_advances_actual_probe_and_clears_spans() {
        let q = Queue::new();
        let guard = q.hold();
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
