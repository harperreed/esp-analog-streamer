// ABOUTME: Runs the production encoder and compressed listener decisions one operation at a time.
// ABOUTME: Leaves sleeps, socket writes, platform metrics and task effects to their callers.
use crate::stream_codec as codec;
use crate::stream_runtime::{health::State, Epoch, Runtime, Worker};
use esp_shazam_core::{
    flac_ring::{self, SourceMetadata},
    media_age::{AgeError, MediaAge},
    stream_buffer::BufferPreset,
    stream_http::ListenerPolicy,
    stream_ring::{Cursor, RawRead, ReadError, StreamRing},
    stream_wait::{BufferedWait, FreshWait, RawWait, SnapshotWait, WaitStep},
};
use std::{io, sync::TryLockError, time::Duration};

/// A listener that has caught up sleeps one FreeRTOS tick: ESP-IDF's usleep busy-waits anything shorter.
pub(crate) const CAUGHT_UP_SLEEP: Duration = Duration::from_millis(10);
/// A caught-up listener waits at most 2 s for real audio, plus one tick and scheduling delay.
/// Quiet audio still produces samples/encoded frames. Capture stalls release both listener slots
/// independently of the capture supervisor; the shared encoder waits again after this timeout.
pub(crate) const FRESH_AUDIO_TIMEOUT: Duration = Duration::from_secs(2);
/// Frames per atomic capture read: at most 5 ms, without holding a capture mutex.
/// WAV packs these samples into 1,440 bytes; the shared encoder accumulates codec-sized blocks.
pub(crate) const CHUNK_FRAMES: usize = 240;

pub(crate) enum EncoderStep {
    Progress,
    Spin,
    SleepUntil(Duration),
    Switched,
    Stopped(io::Error),
}
pub(crate) enum EncoderObservation {
    CursorAcquired {
        generation: u64,
        start: u64,
    },
    AcceptedInput {
        generation: u64,
        start: u64,
        frames: usize,
    },
    RawAttempt {
        result: Result<usize, ReadError>,
        elapsed: Duration,
    },
    RawWait(Duration),
    CursorWaitExpired,
    Stalled,
    Codec(Duration),
    #[cfg(feature = "encoder-probe")]
    PcmPush(Duration),
    Publication {
        lock_wait: Duration,
        held: Duration,
    },
    Reset,
    HealthAck {
        operator_epoch: u32,
    },
}
pub(crate) struct EncoderPump {
    cursor: Option<Cursor>,
    cursor_wait: Option<SnapshotWait>,
    read_wait: Option<(Duration, RawWait)>,
    chunk: Vec<i32>,
    block_number: u32,
    source: Option<SourceMetadata>,
    #[cfg(feature = "encoder-probe")]
    probe_enabled: bool,
}
impl EncoderPump {
    pub(crate) fn new() -> Self {
        Self {
            cursor: None,
            cursor_wait: None,
            read_wait: None,
            chunk: vec![0; 2 * CHUNK_FRAMES],
            block_number: 0,
            source: None,
            #[cfg(feature = "encoder-probe")]
            probe_enabled: false,
        }
    }
    #[cfg(feature = "encoder-probe")]
    pub(crate) fn set_probe(&mut self, enabled: bool) {
        self.probe_enabled = enabled;
    }
    #[cfg(feature = "encoder-probe")]
    pub(crate) fn storage_receipt(&self) -> (usize, usize, usize) {
        (
            self.chunk.as_ptr() as usize,
            self.chunk.len() * std::mem::size_of::<i32>(),
            self.chunk.capacity() * std::mem::size_of::<i32>(),
        )
    }
    pub(crate) fn block_boundary(&self, worker: &Worker) -> bool {
        worker.encoder.frames_wanted() == worker.epoch.format.block_frames()
    }
    pub(crate) fn step(
        &mut self,
        raw: &StreamRing,
        worker: &mut Worker,
        runtime: &Runtime,
        now: &mut impl FnMut() -> Duration,
        observe: &mut impl FnMut(EncoderObservation),
    ) -> EncoderStep {
        self.step_read(raw, worker, runtime, now, observe, |cursor, out| {
            raw.read(cursor, out)
        })
    }
    /// Forwards the existing atomic-copy fault boundary without changing pump decisions.
    #[cfg(all(feature = "capture-fixture", not(target_os = "espidf")))]
    pub(crate) fn step_with_copy_hook(
        &mut self,
        raw: &StreamRing,
        worker: &mut Worker,
        runtime: &Runtime,
        now: &mut impl FnMut() -> Duration,
        observe: &mut impl FnMut(EncoderObservation),
        hook: impl FnOnce(),
    ) -> EncoderStep {
        self.step_read(raw, worker, runtime, now, observe, |cursor, out| {
            raw.read_with_copy_hook(cursor, out, hook)
        })
    }
    fn step_read(
        &mut self,
        raw: &StreamRing,
        worker: &mut Worker,
        runtime: &Runtime,
        now: &mut impl FnMut() -> Duration,
        observe: &mut impl FnMut(EncoderObservation),
        read: impl FnOnce(&mut Cursor, &mut [i32]) -> Result<RawRead, ReadError>,
    ) -> EncoderStep {
        let health_token = runtime.health.token();
        if health_token.state() == State::Terminal {
            if worker.service(runtime) {
                self.cursor = None;
                self.cursor_wait = None;
                self.read_wait = None;
                self.source = None;
                self.block_number = 0;
                return EncoderStep::Switched;
            }
            return EncoderStep::SleepUntil(now() + CAUGHT_UP_SLEEP);
        }
        if health_token.state() == State::ResetRequested {
            if let Err(error) = worker.encoder.reset() {
                return EncoderStep::Stopped(error);
            }
            if worker
                .epoch
                .ring
                .lock()
                .expect("encoded ring lock")
                .reset()
                .is_err()
            {
                return EncoderStep::Stopped(io::Error::other("encoded generation exhausted"));
            }
            self.cursor = None;
            self.cursor_wait = None;
            self.read_wait = None;
            self.source = None;
            self.block_number = 0;
            if runtime.health.transition(health_token, State::Recovering) {
                observe(EncoderObservation::HealthAck {
                    operator_epoch: health_token.operator_epoch(),
                });
            }
            observe(EncoderObservation::Reset);
            return EncoderStep::Progress;
        }
        // Cursor acquisition precedes service, including the initial cursor and reset cursor.
        if self.cursor.is_none() {
            let wait = self.cursor_wait.get_or_insert_with(|| {
                SnapshotWait::new(now() + FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP)
            });
            if !wait.before_attempt(now()) {
                self.cursor_wait = None;
                observe(EncoderObservation::CursorWaitExpired);
                return EncoderStep::Progress;
            }
            return match raw.cursor_now() {
                Ok(cursor) => {
                    let (start, generation) = cursor.position();
                    observe(EncoderObservation::CursorAcquired { generation, start });
                    self.cursor = Some(cursor);
                    self.cursor_wait = None;
                    EncoderStep::Progress
                }
                Err(ReadError::Retry) => EncoderStep::SleepUntil(wait.retry_at(now())),
                Err(ReadError::Overrun) => {
                    self.cursor_wait = None;
                    observe(EncoderObservation::CursorWaitExpired);
                    EncoderStep::Progress
                }
            };
        }
        // Service only at an outer read boundary, never inside a FreshWait retry turn.
        if self.read_wait.is_none() && self.block_boundary(worker) && worker.service(runtime) {
            self.block_number = 0;
            self.source = None;
            self.cursor = None;
            return EncoderStep::Switched;
        }
        let (started, wait) = self.read_wait.get_or_insert_with(|| {
            let started = now();
            (
                started,
                RawWait::new(started, FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP),
            )
        });
        wait.before_attempt(now);
        let frames = CHUNK_FRAMES.min(worker.encoder.frames_wanted());
        let attempt_started = now();
        let (start, generation) = self.cursor.unwrap().position();
        let copied = read(self.cursor.as_mut().unwrap(), &mut self.chunk[..2 * frames]);
        let receipt = copied.ok().and_then(|read| read.receipt);
        let result = copied.map(|read| read.frames);
        observe(EncoderObservation::RawAttempt {
            result,
            elapsed: now().saturating_sub(attempt_started),
        });
        let decision = wait.finish(result, now);
        match decision {
            WaitStep::RetryNow => return EncoderStep::Spin,
            WaitStep::SleepUntil(at) => return EncoderStep::SleepUntil(at),
            _ => observe(EncoderObservation::RawWait(now().saturating_sub(*started))),
        }
        self.read_wait = None;
        match decision {
            // Keep partial codec input during a pause; the next read starts a full budget.
            // Capture reset still produces Overrun and discards it below.
            WaitStep::Stalled | WaitStep::Failed(ReadError::Retry) => {
                observe(EncoderObservation::Stalled);
                return EncoderStep::Progress;
            }
            WaitStep::Failed(ReadError::Overrun) => {
                if worker
                    .epoch
                    .ring
                    .lock()
                    .expect("encoded ring lock")
                    .reset()
                    .is_err()
                {
                    return EncoderStep::Stopped(io::Error::other("encoded generation exhausted"));
                }
                if let Err(error) = worker.encoder.reset() {
                    return EncoderStep::Stopped(error);
                }
                self.cursor = None;
                self.block_number = 0;
                self.source = None;
                observe(EncoderObservation::Reset);
                return EncoderStep::Progress;
            }
            WaitStep::Ready(n) => {
                let receipt = receipt.expect("nonempty raw read carries metadata");
                self.source = Some(match self.source {
                    Some(mut source) => {
                        assert_eq!(source.raw_generation, receipt.generation);
                        assert_eq!(source.source_end, receipt.source_start);
                        source.source_end = receipt.source_end;
                        source.oldest_accepted_us =
                            source.oldest_accepted_us.min(receipt.oldest_accepted_us);
                        source.raw_publication_us = receipt.committed_at_us;
                        source
                    }
                    None => SourceMetadata {
                        raw_generation: receipt.generation,
                        source_start: receipt.source_start,
                        source_end: receipt.source_end,
                        oldest_accepted_us: receipt.oldest_accepted_us,
                        raw_publication_us: receipt.committed_at_us,
                    },
                });
                observe(EncoderObservation::AcceptedInput {
                    generation,
                    start,
                    frames: n,
                });
                #[cfg(feature = "encoder-probe")]
                let push_started = self.probe_enabled.then(&mut *now);
                worker.encoder.push(&self.chunk[..2 * n]);
                #[cfg(feature = "encoder-probe")]
                if let Some(started) = push_started {
                    observe(EncoderObservation::PcmPush(now().saturating_sub(started)));
                }
            }
            _ => unreachable!(),
        }
        if worker.encoder.frames_wanted() == 0 {
            // The observation contains only the codec call, including preemption.
            let started = now();
            let result = worker
                .encoder
                .encode_block(self.block_number, &mut worker.bytes);
            observe(EncoderObservation::Codec(now().saturating_sub(started)));
            if let Err(error) = result {
                if worker
                    .epoch
                    .ring
                    .lock()
                    .expect("encoded ring lock")
                    .reset()
                    .is_err()
                {
                    return EncoderStep::Stopped(io::Error::other("encoded generation exhausted"));
                }
                return EncoderStep::Stopped(error);
            }
            let started = now();
            let mut output = worker.epoch.ring.lock().expect("encoded ring lock");
            let acquired = now();
            if !runtime.health.publication_available() {
                return EncoderStep::Stopped(io::Error::other("output receipt identity exhausted"));
            }
            let receipt = match output.push(
                &worker.bytes,
                self.source
                    .take()
                    .expect("complete codec block has source metadata"),
                || now().as_micros() as u64,
            ) {
                Ok(receipt) => receipt,
                Err(_) => {
                    return EncoderStep::Stopped(io::Error::other(
                        "encoded frame or publication identity invalid",
                    ))
                }
            };
            runtime.health.publish(
                worker
                    .epoch
                    .operator_epoch
                    .load(std::sync::atomic::Ordering::Acquire),
                worker.epoch.format,
                receipt,
            );
            let finished = now();
            drop(output);
            self.block_number = self.block_number.wrapping_add(1);
            observe(EncoderObservation::Publication {
                lock_wait: acquired.saturating_sub(started),
                held: finished.saturating_sub(acquired),
            });
        }
        EncoderStep::Progress
    }
}

/// Why a listener's turn ends before its socket does, decided in one place: a revoked slot is
/// `Retired` and outranks a stale network lease, `NetworkChanged`. The listener loops ask this
/// at the top of each turn and after each socket write, and their waits cancel on it.
pub(crate) struct DepartureGuard<'a> {
    pub(crate) slot: &'a esp_shazam_core::stream_wait::Slot,
    pub(crate) network: &'a esp_shazam_core::network_lifecycle::Network,
    pub(crate) lease: esp_shazam_core::network_lifecycle::Lease,
}
impl DepartureGuard<'_> {
    /// `Slot::revoked` refreshes the capacity registry first, as every caller relied on.
    pub(crate) fn departed(&self) -> Option<End> {
        if self.slot.revoked() {
            Some(End::Retired)
        } else if !self.network.is_current(self.lease) {
            Some(End::NetworkChanged)
        } else {
            None
        }
    }
}

/// Why a listener's stream ended.
#[derive(Debug)]
pub(crate) enum End {
    Retired,
    NetworkChanged,
    /// The ring overwrote frames the listener hadn't been sent: it read too slowly.
    FellBehind,
    /// Writing a frame exhausted its write budget: the listener stopped reading, or reads too
    /// little to matter.
    StoppedReading,
    /// Capture or the shared encoder stopped supplying fresh audio.
    CaptureStalled,
    /// A copied shared frame failed its codec-specific listener preparation.
    InvalidFrame(io::Error),
    /// The socket failed, usually because the listener closed it.
    Failed(io::Error),
    EncodedReset,
    InvalidCursor,
    MediaAge,
    EncoderUnhealthy,
}
impl End {
    pub(crate) fn from_encoded_read_error(error: flac_ring::ReadError) -> Self {
        match error {
            flac_ring::ReadError::GenerationChanged => Self::EncodedReset,
            flac_ring::ReadError::Overwritten => Self::FellBehind,
            flac_ring::ReadError::InvalidOffset => Self::InvalidCursor,
        }
    }
    pub(crate) fn from_write_error(error: io::Error) -> Self {
        // lwIP reports a send timeout as EAGAIN; the deadline running out is TimedOut.
        match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => Self::StoppedReading,
            _ => Self::Failed(error),
        }
    }
}
pub(crate) enum ListenerStep {
    Progress,
    SleepUntil(Duration),
    Write { deadline: Duration, header: bool },
    End(End),
}
enum Admission {
    Live(SnapshotWait),
    Buffered { wait: BufferedWait, frames: usize },
}
enum Phase {
    Admission(Admission),
    Header,
    Frame,
    Read {
        started: Duration,
        wait: FreshWait,
    },
    Write {
        deadline: Duration,
        header: bool,
        len: usize,
    },
    End,
}
/// Why a live listener owes a cursor move before its next read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OwedSkip {
    /// A stale frame was dropped unwritten; that drop is already on the counters.
    Dropped,
    /// A frame went out complete but late; nothing is counted until the move passes frames.
    Late,
}
pub(crate) struct CompressedListener {
    phase: Phase,
    cursor: Option<flac_ring::Cursor>,
    header: Vec<u8>,
    /// The header write's budget, fixed; a frame write's budget depends on the policy.
    header_limit: Duration,
    write_limit: Duration,
    block_number: u32,
    pub(crate) refuse_buffered: bool,
    pub(crate) read_wait: Duration,
    pub(crate) frames_sent: u64,
    pub(crate) bytes_sent: u64,
    age: MediaAge,
    policy: ListenerPolicy,
    /// A live listener owes a cursor move before its next read, after a stale or late frame.
    pending_skip: Option<OwedSkip>,
    pending_source: Option<SourceMetadata>,
    pending_receipt: Option<flac_ring::FrameReceipt>,
    pub(crate) first_sent: Option<flac_ring::FrameReceipt>,
    pub(crate) last_sent: Option<flac_ring::FrameReceipt>,
    pub(crate) last_age_us: u64,
    pub(crate) max_age_us: u64,
    pub(crate) queue_frames_max: u64,
    /// Live-policy resynchronizations: events, whole frames passed over, and the last event's time.
    pub(crate) skips: u64,
    pub(crate) frames_skipped: u64,
    pub(crate) last_skip_us: u64,
}
impl CompressedListener {
    pub(crate) fn new(
        epoch: &Epoch,
        preset: Option<BufferPreset>,
        recognition: bool,
        policy: ListenerPolicy,
        now: &mut impl FnMut() -> Duration,
    ) -> Self {
        let mut header = epoch.format.format().response_head().as_bytes().to_vec();
        header.extend_from_slice(&codec::stream_header(epoch.format));
        let started = now();
        let admission = if let Some(preset) = preset {
            Admission::Buffered {
                wait: BufferedWait::new(
                    started,
                    started + Duration::from_secs(preset.seconds()) + FRESH_AUDIO_TIMEOUT,
                    FRESH_AUDIO_TIMEOUT,
                    CAUGHT_UP_SLEEP,
                ),
                frames: preset.frames(epoch.format.block_frames()),
            }
        } else {
            Admission::Live(SnapshotWait::new(
                started + FRESH_AUDIO_TIMEOUT,
                CAUGHT_UP_SLEEP,
            ))
        };
        Self {
            phase: Phase::Admission(admission),
            cursor: None,
            header,
            header_limit: Duration::from_millis(epoch.format.write_limit_ms(recognition)),
            // A live frame may take twice as long as the ring holds audio; a socket that accepts
            // nothing for longer is dead, not slow. See OutputFormat::live_write_limit_ms.
            write_limit: Duration::from_millis(match policy {
                ListenerPolicy::Live => epoch.format.live_write_limit_ms(recognition),
                ListenerPolicy::Exact => epoch.format.write_limit_ms(recognition),
            }),
            block_number: 0,
            refuse_buffered: false,
            read_wait: Duration::ZERO,
            frames_sent: 0,
            bytes_sent: 0,
            age: MediaAge::for_stream(preset, epoch.format),
            policy,
            pending_skip: None,
            pending_source: None,
            pending_receipt: None,
            first_sent: None,
            last_sent: None,
            last_age_us: 0,
            max_age_us: 0,
            queue_frames_max: 0,
            skips: 0,
            frames_skipped: 0,
            last_skip_us: 0,
        }
    }
    fn end(&mut self, end: End) -> ListenerStep {
        self.phase = Phase::End;
        ListenerStep::End(end)
    }
    /// A live listener owes a move before its next read; an exact one ends. A dropped frame is
    /// counted here, at drop time, so a departure before that read still accounts for it.
    fn lagged(&mut self, owed: OwedSkip, at_us: u64) -> ListenerStep {
        match self.policy {
            ListenerPolicy::Exact => self.end(End::MediaAge),
            ListenerPolicy::Live => {
                if owed == OwedSkip::Dropped {
                    self.skips += 1;
                    self.frames_skipped += 1;
                    self.last_skip_us = at_us;
                }
                self.pending_skip = Some(owed);
                self.phase = Phase::Frame;
                ListenerStep::Progress
            }
        }
    }
    /// One turn of the listener loop: the departure guard first, then `step`. The loop asks the
    /// guard again after each socket write.
    pub(crate) fn step_guarded(
        &mut self,
        epoch: &Epoch,
        scratch: &mut [u8],
        now: &mut impl FnMut() -> Duration,
        peer_connected: &mut impl FnMut() -> bool,
        guard: &DepartureGuard<'_>,
    ) -> ListenerStep {
        if let Some(end) = guard.departed() {
            return self.end(end);
        }
        self.step(epoch, scratch, now, peer_connected)
    }
    pub(crate) fn step(
        &mut self,
        epoch: &Epoch,
        scratch: &mut [u8],
        now: &mut impl FnMut() -> Duration,
        peer_connected: &mut impl FnMut() -> bool,
    ) -> ListenerStep {
        if !epoch.retired() && !epoch.healthy() {
            return self.end(End::EncoderUnhealthy);
        }
        match &mut self.phase {
            Phase::Admission(admission) => {
                let at = now();
                let allowed = match admission {
                    Admission::Live(wait) => wait.before_attempt(at),
                    Admission::Buffered { wait, .. } => wait.before_attempt(at),
                };
                if !allowed {
                    self.refuse_buffered = matches!(admission, Admission::Buffered { .. });
                    return self.end(End::CaptureStalled);
                }
                if matches!(admission, Admission::Buffered { .. }) && !peer_connected() {
                    return self.end(End::CaptureStalled);
                }
                match epoch.ring.try_lock() {
                    Ok(ring) => {
                        let cursor = match admission {
                            Admission::Live(_) => Some(ring.cursor_now()),
                            Admission::Buffered { wait, frames } => {
                                wait.observe_progress(ring.progress(), now);
                                match wait.retained(ring.cursor_retained(*frames)) {
                                    Ok(cursor) => cursor,
                                    Err(_) => {
                                        self.refuse_buffered = true;
                                        return self.end(End::CaptureStalled);
                                    }
                                }
                            }
                        };
                        if let Some(cursor) = cursor {
                            self.cursor = Some(cursor);
                            self.phase = Phase::Header;
                            return ListenerStep::Progress;
                        }
                    }
                    Err(TryLockError::WouldBlock) => {}
                    Err(TryLockError::Poisoned(_)) => panic!("stream ring lock poisoned"),
                }
                ListenerStep::SleepUntil(now() + CAUGHT_UP_SLEEP)
            }
            Phase::Header => {
                if epoch.retired() {
                    return self.end(End::Retired);
                }
                let deadline = now() + self.header_limit;
                self.phase = Phase::Write {
                    deadline,
                    header: true,
                    len: self.header.len(),
                };
                ListenerStep::Write {
                    deadline,
                    header: true,
                }
            }
            Phase::Frame => {
                if epoch.retired() {
                    return self.end(End::Retired);
                }
                let started = now();
                self.phase = Phase::Read {
                    started,
                    wait: FreshWait::new(started, FRESH_AUDIO_TIMEOUT, CAUGHT_UP_SLEEP),
                };
                self.step(epoch, scratch, now, peer_connected)
            }
            Phase::Read { started, wait } => {
                let result = match epoch.ring.try_lock() {
                    Ok(ring) => {
                        if epoch.retired() {
                            Err(End::Retired)
                        } else {
                            let mut cursor = self.cursor.expect("reading listener holds a cursor");
                            // One resynchronization of `frames` whole frames; nothing moved is
                            // no skip. The three counters are fixed-size fields: no allocation.
                            let mut record_skip = |frames: u64, event: bool, at: Duration| {
                                if frames > 0 {
                                    if event {
                                        self.skips += 1;
                                    }
                                    self.frames_skipped += frames;
                                    self.last_skip_us = at.as_micros() as u64;
                                }
                            };
                            // A move owed from a stale or late frame happens under this lock, at
                            // a frame boundary, and records the lag it finds. A reset ends the
                            // listener before anything more is counted.
                            let owed = self.pending_skip.take();
                            let caught_up = match owed {
                                Some(_) => ring.catch_up(&mut cursor),
                                None => Ok(0),
                            };
                            let mut read = match caught_up {
                                Ok(passed) => {
                                    self.queue_frames_max = self.queue_frames_max.max(passed);
                                    // A drop is already counted: only the frames passed are added.
                                    if let Some(owed) = owed {
                                        record_skip(passed, owed == OwedSkip::Late, now());
                                    }
                                    ring.read(&mut cursor, scratch)
                                }
                                Err(error) => Err(error),
                            };
                            if self.policy == ListenerPolicy::Live
                                && read == Err(flac_ring::ReadError::Overwritten)
                            {
                                // Lapped: record the lag, move to the newest whole frame under
                                // the same lock, and read it now.
                                if let Ok(passed) = ring.catch_up(&mut cursor) {
                                    self.queue_frames_max = self.queue_frames_max.max(passed);
                                    record_skip(passed, true, now());
                                    read = ring.read(&mut cursor, scratch);
                                }
                            }
                            self.cursor = Some(cursor);
                            read.map(|read| {
                                if let Some(receipt) = read.receipt {
                                    self.queue_frames_max = self
                                        .queue_frames_max
                                        .max(ring.progress().1.saturating_sub(receipt.sequence));
                                }
                                self.pending_receipt = read.receipt;
                                self.pending_source =
                                    read.receipt.map(|receipt| receipt.metadata.source);
                                read.bytes
                            })
                            .map_err(End::from_encoded_read_error)
                        }
                    }
                    Err(TryLockError::WouldBlock) => Ok(0),
                    Err(TryLockError::Poisoned(_)) => panic!("stream ring lock poisoned"),
                };
                let n = match wait.finish(result, now) {
                    WaitStep::SleepUntil(at) => return ListenerStep::SleepUntil(at),
                    WaitStep::Ready(n) => n,
                    WaitStep::Stalled => {
                        self.read_wait += now().saturating_sub(*started);
                        return self.end(End::CaptureStalled);
                    }
                    WaitStep::Failed(end) => {
                        self.read_wait += now().saturating_sub(*started);
                        return self.end(end);
                    }
                    WaitStep::RetryNow => unreachable!(),
                };
                self.read_wait += now().saturating_sub(*started);
                let n = match codec::prepare_listener_frame(
                    epoch.format,
                    scratch,
                    n,
                    self.block_number,
                ) {
                    Ok(n) => n,
                    Err(error) => return self.end(End::InvalidFrame(error)),
                };
                let at = now();
                let source = self
                    .pending_source
                    .expect("encoded frame carries source metadata");
                let produced = self.pending_receipt.unwrap().metadata.encoded_production_us;
                if source.source_start >= source.source_end
                    || source.oldest_accepted_us > source.raw_publication_us
                    || source.raw_publication_us > produced
                    || produced > at.as_micros() as u64
                {
                    return self.end(End::MediaAge);
                }
                let socket_deadline = at + self.write_limit;
                let deadline = match self.age.deadline(
                    at.as_micros() as u64,
                    source.oldest_accepted_us,
                    socket_deadline.as_micros() as u64,
                ) {
                    // A live write gets the whole socket budget: the media bound is a reason to
                    // skip, never to cut a frame short on the wire.
                    Ok(_) if self.policy == ListenerPolicy::Live => socket_deadline,
                    Ok(deadline) => Duration::from_micros(deadline),
                    // The stale frame is dropped unwritten.
                    Err(AgeError::Stale) => {
                        return self.lagged(OwedSkip::Dropped, at.as_micros() as u64)
                    }
                    Err(_) => return self.end(End::MediaAge),
                };
                self.phase = Phase::Write {
                    deadline,
                    header: false,
                    len: n,
                };
                ListenerStep::Write {
                    deadline,
                    header: false,
                }
            }
            Phase::Write {
                deadline, header, ..
            } => ListenerStep::Write {
                deadline: *deadline,
                header: *header,
            },
            Phase::End => panic!("listener stepped after termination"),
        }
    }
    pub(crate) fn pending_oldest_accepted_us(&self) -> Option<u64> {
        match self.phase {
            Phase::Write { header: false, .. } => {
                self.pending_source.map(|source| source.oldest_accepted_us)
            }
            _ => None,
        }
    }
    /// The budget the pending write was given: the header's fixed limit, or the frame budget the
    /// policy set (ring depth for a live reader). Timeout receipts label themselves with it.
    pub(crate) fn pending_write_limit(&self) -> Duration {
        match self.phase {
            Phase::Write { header: true, .. } => self.header_limit,
            Phase::Write { .. } => self.write_limit,
            _ => panic!("listener has no pending write"),
        }
    }
    pub(crate) fn write_bytes<'a>(&'a self, scratch: &'a [u8]) -> &'a [u8] {
        match self.phase {
            Phase::Write { header: true, .. } => &self.header,
            Phase::Write { len, .. } => &scratch[..len],
            _ => panic!("listener has no pending write"),
        }
    }
    pub(crate) fn write_finished(
        &mut self,
        result: io::Result<()>,
        now: &mut impl FnMut() -> Duration,
    ) -> ListenerStep {
        let Phase::Write { header, len, .. } = self.phase else {
            panic!("listener has no pending write")
        };
        if let Err(error) = result {
            // A partial frame on the wire is a dead socket under either policy; only an exact
            // reader also reports the media bound it was past.
            if !header
                && self.policy == ListenerPolicy::Exact
                && self
                    .age
                    .check(
                        now().as_micros() as u64,
                        self.pending_source.unwrap().oldest_accepted_us,
                    )
                    .is_err()
            {
                return self.end(End::MediaAge);
            }
            return self.end(End::from_write_error(error));
        }
        if !header {
            self.frames_sent += 1;
            self.bytes_sent += len as u64;
            if self.first_sent.is_none() {
                self.first_sent = self.pending_receipt;
            }
            self.last_sent = self.pending_receipt;
            self.last_age_us = (now().as_micros() as u64)
                .saturating_sub(self.pending_source.unwrap().oldest_accepted_us);
            self.max_age_us = self.max_age_us.max(self.last_age_us);
            // FLAC numbering advances after a whole write and rejects numbers at 2^31.
            // MP3 ignores this counter, so wrap cannot alter its frame bytes.
            self.block_number = self.block_number.wrapping_add(1);
            let at_us = now().as_micros() as u64;
            match self
                .age
                .check(at_us, self.pending_source.unwrap().oldest_accepted_us)
            {
                Ok(()) => {}
                // Complete but late: the frame counts, and a live listener skips before reading on.
                Err(AgeError::Stale) => return self.lagged(OwedSkip::Late, at_us),
                Err(_) => return self.end(End::MediaAge),
            }
        }
        self.phase = Phase::Frame;
        ListenerStep::Progress
    }
}

/// Owns admission authority until the actual synchronous socket close succeeds.
pub(crate) struct ConnectionResources<S> {
    pub(crate) socket: S,
    pub(crate) slot: esp_shazam_core::stream_wait::Slot,
    pub(crate) lease: Option<crate::stream_runtime::Lease>,
}
/// A socket whose admission guards are released and whose checked close is still owed.
/// Its close bookkeeping travels with it to the close reaper.
pub(crate) struct DrainingSocket<S> {
    pub(crate) socket: S,
    pub(crate) owner_id: u64,
    pub(crate) sonos_intent: u64,
    /// Slots held at the release, counting this socket's.
    pub(crate) held: usize,
    /// When the slot was released, stamped before the owner record reset so a replacement's
    /// join never precedes it.
    pub(crate) released_us: u64,
}
/// An uncertain close retains the concrete failure, which owns the socket when close was
/// never called. The caller applies its recovery policy; the slot was released at hand-off.
#[must_use]
pub(crate) struct RetainedClose<F> {
    _failure: F,
}
impl<S> ConnectionResources<S> {
    pub(crate) fn new(
        socket: S,
        slot: esp_shazam_core::stream_wait::Slot,
        lease: Option<crate::stream_runtime::Lease>,
    ) -> Self {
        Self {
            socket,
            slot,
            lease,
        }
    }
    /// Releases both admission guards and returns the socket with its close bookkeeping.
    /// Call this only once a draining place is reserved, so the socket cannot be lost.
    pub(crate) fn release(self, now: impl FnOnce() -> u64) -> DrainingSocket<S> {
        let Self {
            socket,
            slot,
            lease,
        } = self;
        let owner_id = slot.owner_id();
        let sonos_intent = slot.intent_id();
        let held = slot.taken();
        let released_us = now();
        drop(slot);
        drop(lease);
        DrainingSocket {
            socket,
            owner_id,
            sonos_intent,
            held,
            released_us,
        }
    }
}
impl<S> DrainingSocket<S> {
    /// Closes exactly once. A failure is retained, never retried, because the socket's state
    /// is unknown.
    pub(crate) fn close<R, F>(
        self,
        close_once: impl FnOnce(S) -> Result<R, F>,
    ) -> Result<R, RetainedClose<F>> {
        close_once(self.socket).map_err(|failure| RetainedClose { _failure: failure })
    }
}

/// Runs the unchanged eight-publication delay, with diagnostic-only report ordering.
pub(crate) fn finish_encoder_turn<C>(
    published: bool,
    format: esp_shazam_core::stream_buffer::OutputFormat,
    yield_frames: &mut usize,
    context: &mut C,
    report: impl FnOnce(&mut C),
    delay: impl FnOnce(&mut C),
) {
    #[cfg(not(feature = "encoder-probe"))]
    report(context);
    if published && format == esp_shazam_core::stream_buffer::OutputFormat::Mp3 {
        *yield_frames += 1;
        if *yield_frames == 8 {
            delay(context);
            *yield_frames = 0;
        }
    }
    #[cfg(feature = "encoder-probe")]
    report(context);
}
