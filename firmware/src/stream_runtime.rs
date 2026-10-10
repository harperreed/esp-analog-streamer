// ABOUTME: Coordinates one encoder worker with immutable compressed stream epochs.
// ABOUTME: Prepares replacements before a bounded, cancellable commit and retires old readers.
use crate::stream_codec::Encoder;
use esp_shazam_core::{
    flac_ring::FlacRing,
    stream_buffer::OutputFormat,
    stream_http::{self, Route},
};
#[cfg(feature = "listener-loom")]
use loom::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
#[cfg(not(feature = "listener-loom"))]
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::{
    io,
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

pub struct Epoch {
    pub format: OutputFormat,
    pub ring: Mutex<FlacRing>,
    retired: AtomicBool,
}
impl Epoch {
    pub fn retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }
}
pub struct Lease {
    pub epoch: Arc<Epoch>,
    listeners: Arc<AtomicUsize>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.listeners.fetch_sub(1, Ordering::Release);
    }
}
struct Prepared {
    epoch: Arc<Epoch>,
    encoder: Encoder,
    bytes: Vec<u8>,
}
impl Prepared {
    fn new(format: OutputFormat, recognition: bool) -> io::Result<Self> {
        let ring = FlacRing::try_new(format.history_frames(recognition), format.max_frame_bytes())
            .map_err(|e| io::Error::other(format!("compressed ring allocation failed: {e:?}")))?;
        let encoder = Encoder::new(format)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(format.max_frame_bytes())
            .map_err(io::Error::other)?;
        Ok(Self {
            epoch: Arc::new(Epoch {
                format,
                ring: Mutex::new(ring),
                retired: AtomicBool::new(false),
            }),
            encoder,
            bytes,
        })
    }
}
struct Request {
    prepared: Prepared,
    force: bool,
    token: Arc<Mutex<bool>>,
    deadline: Duration,
    outcome: Arc<Mutex<Option<io::Result<bool>>>>,
    notify: SyncSender<()>,
}
pub struct Runtime {
    active: Mutex<Arc<Epoch>>,
    sender: SyncSender<Request>,
    recognition: bool,
    listeners: Arc<AtomicUsize>,
    clock: std::sync::Arc<dyn Fn() -> Duration + Send + Sync>,
}
pub struct Worker {
    pub epoch: Arc<Epoch>,
    pub encoder: Encoder,
    pub bytes: Vec<u8>,
    receiver: Receiver<Request>,
}
impl Runtime {
    pub fn new(format: OutputFormat, recognition: bool) -> io::Result<(Arc<Self>, Worker)> {
        let origin = Instant::now();
        Self::with_clock(
            format,
            recognition,
            std::sync::Arc::new(move || origin.elapsed()),
        )
    }
    pub(crate) fn with_clock(
        format: OutputFormat,
        recognition: bool,
        clock: std::sync::Arc<dyn Fn() -> Duration + Send + Sync>,
    ) -> io::Result<(Arc<Self>, Worker)> {
        format.validate(recognition).map_err(io::Error::other)?;
        let prepared = Prepared::new(format, recognition)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let runtime = Arc::new(Self {
            active: Mutex::new(prepared.epoch.clone()),
            sender,
            recognition,
            clock,
            listeners: Arc::new(AtomicUsize::new(0)),
        });
        Ok((
            runtime,
            Worker {
                epoch: prepared.epoch,
                encoder: prepared.encoder,
                bytes: prepared.bytes,
                receiver,
            },
        ))
    }
    pub fn active_format(&self) -> OutputFormat {
        self.active.lock().unwrap().format
    }
    /// Admission and the idle commit predicate share this lock. The lease survives socket close.
    pub fn admit(&self, line: &str) -> (Route, Option<Lease>) {
        let active = self.active.lock().unwrap();
        let route =
            stream_http::route_for(line, active.format).with_buffer_availability(!self.recognition);
        let compressed = matches!(
            route,
            Route::Buffered(_)
                | Route::Stream(stream_http::Format::Flac | stream_http::Format::Mp3)
        );
        let lease = compressed.then(|| {
            self.listeners.fetch_add(1, Ordering::Relaxed);
            Lease {
                epoch: active.clone(),
                listeners: self.listeners.clone(),
            }
        });
        (route, lease)
    }
    pub fn switch(
        &self,
        format: OutputFormat,
        force: bool,
        token: Arc<Mutex<bool>>,
        timeout: Duration,
    ) -> io::Result<bool> {
        match self.begin_switch(format, force, token, timeout) {
            SwitchStart::Finished(result) => result,
            SwitchStart::Pending(pending) => pending.wait(),
        }
    }
    pub(crate) fn begin_switch(
        &self,
        format: OutputFormat,
        force: bool,
        token: Arc<Mutex<bool>>,
        timeout: Duration,
    ) -> SwitchStart {
        if let Err(error) = format.validate(self.recognition) {
            return SwitchStart::Finished(Err(io::Error::other(error)));
        }
        if self.active_format() == format {
            return SwitchStart::Finished(Ok(true));
        }
        self.begin_prepared(force, token, timeout, || {
            Prepared::new(format, self.recognition)
        })
    }
    /// Every deliberate Play waits for capacity, including a retry of the already active codec.
    pub fn play_format(
        &self,
        format: OutputFormat,
        token: Arc<Mutex<bool>>,
        listeners: &AtomicUsize,
        limit: usize,
        wait_timeout: Duration,
    ) -> io::Result<bool> {
        let applied = self.switch(format, true, token.clone(), Duration::from_secs(3))?;
        let deadline = Instant::now() + wait_timeout;
        loop {
            if !*token.lock().unwrap() {
                return Err(io::Error::other("Sonos Play cancelled"));
            }
            if listeners.load(Ordering::Acquire) < limit {
                return Ok(applied);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "stream listeners did not release a slot",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(test)]
    fn switch_prepared(
        &self,
        _format: OutputFormat,
        force: bool,
        token: Arc<Mutex<bool>>,
        timeout: Duration,
        prepare: impl FnOnce() -> io::Result<Prepared>,
    ) -> io::Result<bool> {
        match self.begin_prepared(force, token, timeout, prepare) {
            SwitchStart::Finished(result) => result,
            SwitchStart::Pending(pending) => pending.wait(),
        }
    }
    fn begin_prepared(
        &self,
        force: bool,
        token: Arc<Mutex<bool>>,
        timeout: Duration,
        prepare: impl FnOnce() -> io::Result<Prepared>,
    ) -> SwitchStart {
        let deadline = (self.clock)() + timeout;
        let prepared = match prepare() {
            Ok(prepared) => prepared,
            Err(error) => return SwitchStart::Finished(Err(error)),
        };
        let outcome = Arc::new(Mutex::new(None));
        let (notify, notified) = mpsc::sync_channel(1);
        if self
            .sender
            .try_send(Request {
                prepared,
                force,
                token,
                deadline,
                outcome: outcome.clone(),
                notify,
            })
            .is_err()
        {
            return SwitchStart::Finished(Err(io::Error::other(
                "encoder switch busy or unavailable",
            )));
        }
        SwitchStart::Pending(PendingSwitch {
            notified,
            outcome,
            deadline,
            clock: self.clock.clone(),
        })
    }
}
pub(crate) enum SwitchStart {
    Finished(io::Result<bool>),
    Pending(PendingSwitch),
}
#[cfg(not(target_os = "espidf"))]
#[allow(dead_code)] // Host test binaries import different subsets of this production module.
pub(crate) enum SwitchPoll {
    Finished(io::Result<bool>),
    Pending(PendingSwitch),
}
pub(crate) struct PendingSwitch {
    notified: Receiver<()>,
    outcome: Arc<Mutex<Option<io::Result<bool>>>>,
    deadline: Duration,
    clock: std::sync::Arc<dyn Fn() -> Duration + Send + Sync>,
}
impl PendingSwitch {
    #[cfg(not(target_os = "espidf"))]
    #[allow(dead_code)] // The cooperative host adapter is absent from blocking-only test binaries.
    pub(crate) fn poll(self) -> SwitchPoll {
        match self.notified.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => SwitchPoll::Finished(self.finalize()),
            Err(mpsc::TryRecvError::Empty) if (self.clock)() >= self.deadline => {
                SwitchPoll::Finished(self.finalize())
            }
            Err(mpsc::TryRecvError::Empty) => SwitchPoll::Pending(self),
        }
    }
    fn wait(self) -> io::Result<bool> {
        let _ = self
            .notified
            .recv_timeout(self.deadline.saturating_sub((self.clock)()));
        self.finalize()
    }
    pub(crate) fn finalize(self) -> io::Result<bool> {
        let mut result = self.outcome.lock().unwrap();
        // This lock serializes expiry with the worker's commit: a timeout revokes authority.
        result.take().unwrap_or_else(|| {
            *result = Some(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "encoder switch cancelled after deadline",
            )));
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "encoder switch deadline exceeded",
            ))
        })
    }
}

impl Worker {
    /// Called between complete PCM blocks; only this worker ever executes an encoder.
    pub fn service(&mut self, runtime: &Runtime) -> bool {
        match self.receiver.try_recv() {
            Ok(request) => self.commit(runtime, request),
            Err(_) => false,
        }
    }
    #[cfg(test)]
    pub fn wait_and_service(&mut self, runtime: &Runtime, timeout: Duration) -> bool {
        match self.receiver.recv_timeout(timeout) {
            Ok(request) => self.commit(runtime, request),
            Err(_) => false,
        }
    }
    fn commit(&mut self, runtime: &Runtime, request: Request) -> bool {
        let permit = request.token.lock().unwrap();
        let mut outcome = request.outcome.lock().unwrap();
        let mut active = runtime.active.lock().unwrap();
        let result = if outcome.is_some() || !*permit || (runtime.clock)() >= request.deadline {
            Err(io::Error::other("encoder switch cancelled"))
        } else if !request.force && runtime.listeners.load(Ordering::Acquire) != 0 {
            Ok(false)
        } else {
            active.retired.store(true, Ordering::Release);
            *active = request.prepared.epoch.clone();
            self.epoch = request.prepared.epoch;
            self.encoder = request.prepared.encoder;
            self.bytes = request.prepared.bytes;
            Ok(true)
        };
        let committed = matches!(result, Ok(true));
        *outcome = Some(result);
        let _ = request.notify.try_send(());
        committed
    }
}

#[cfg(all(test, not(feature = "listener-loom")))]
mod tests {
    use super::*;
    #[test]
    fn idle_commit_rechecks_listener_admitted_during_preparation() {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let caller = runtime.clone();
        let (ready, prepared) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let join = std::thread::spawn(move || {
            caller.switch_prepared(
                OutputFormat::Mp3,
                false,
                Arc::new(Mutex::new(true)),
                Duration::from_secs(1),
                || {
                    let replacement = Prepared::new(OutputFormat::Mp3, false)?;
                    ready.send(()).unwrap();
                    resumed.recv().unwrap();
                    Ok(replacement)
                },
            )
        });
        prepared.recv_timeout(Duration::from_secs(1)).unwrap();
        let (_, listener) = runtime.admit("GET /stream.flac HTTP/1.1");
        resume.send(()).unwrap();
        assert!(!worker.wait_and_service(&runtime, Duration::from_secs(1)));
        assert!(!join.join().unwrap().unwrap());
        assert!(!listener.unwrap().epoch.retired());
        assert_eq!(runtime.active_format(), OutputFormat::Flac);
    }

    #[test]
    fn actual_storage_allocation_failure_keeps_encoder_and_ring_usable() {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
        let failed = runtime.switch_prepared(
            OutputFormat::Flac,
            true,
            Arc::new(Mutex::new(true)),
            Duration::from_secs(1),
            || {
                FlacRing::try_new(1, usize::MAX)
                    .map_err(|e| io::Error::other(format!("allocation failed: {e:?}")))?;
                unreachable!()
            },
        );
        assert!(failed.is_err());
        assert_eq!(runtime.active_format(), OutputFormat::Mp3);
        assert_eq!(worker.encoder.frames_wanted(), 1152);
        worker
            .encoder
            .push(&vec![0; 2 * OutputFormat::Mp3.block_frames()]);
        worker.encoder.encode_block(0, &mut worker.bytes).unwrap();
        worker
            .epoch
            .ring
            .lock()
            .unwrap()
            .push(&worker.bytes)
            .unwrap();
        assert_eq!(worker.epoch.ring.lock().unwrap().progress(), (0, 1));
        assert!(!worker.service(&runtime));
        assert_eq!(crate::stream_codec::SAMPLE_RATE_HZ, 48_000);
        let mut bytes = worker.bytes.clone();
        assert_eq!(
            crate::stream_codec::prepare_listener_frame(OutputFormat::Mp3, &mut bytes, 960, 0)
                .unwrap(),
            960
        );
        assert!(crate::stream_codec::stream_header(OutputFormat::Mp3).is_empty());
        worker.encoder.reset().unwrap();
    }
}

#[cfg(all(test, feature = "listener-loom"))]
#[path = "../../core/tests/support/listener_runtime_models.rs"]
mod models;
