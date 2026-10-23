// ABOUTME: Waits for fresh stream data and holds listener slots until their sockets close.
// ABOUTME: Shares the firmware's producer-wait and slot-release behavior with real host tests.

#[cfg(all(feature = "listener-loom", target_os = "espidf"))]
compile_error!("listener-loom is host-only");
#[cfg(all(feature = "listener-loom", feature = "capture-loom"))]
compile_error!("listener-loom and capture-loom require separate model invocations");
#[cfg(feature = "listener-loom")]
use loom::sync::{Arc, Mutex as CapacityMutex};
#[cfg(not(feature = "listener-loom"))]
use std::sync::{Arc, Mutex as CapacityMutex};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum WaitError<E> {
    Stalled,
    Read(E),
}

/// Result of one real read attempt; callers perform the requested sleep or spin.
#[derive(Debug, PartialEq, Eq)]
pub enum WaitStep<E> {
    Ready(usize),
    Failed(E),
    Stalled,
    RetryNow,
    SleepUntil(Duration),
}

pub struct FreshWait {
    started: Duration,
    limit: Duration,
    interval: Duration,
}
impl FreshWait {
    pub fn new(started: Duration, limit: Duration, interval: Duration) -> Self {
        assert!(!limit.is_zero() && !interval.is_zero());
        Self {
            started,
            limit,
            interval,
        }
    }
    pub fn finish<E>(
        &self,
        result: Result<usize, E>,
        now: &mut impl FnMut() -> Duration,
    ) -> WaitStep<E> {
        match result {
            Ok(0) => {
                let finished = now();
                if finished.saturating_sub(self.started) >= self.limit {
                    WaitStep::Stalled
                } else {
                    WaitStep::SleepUntil(finished + self.interval)
                }
            }
            Ok(n) => WaitStep::Ready(n),
            Err(error) => WaitStep::Failed(error),
        }
    }
}

pub struct RawWait {
    fresh: FreshWait,
    retry_started: Option<Duration>,
}
impl RawWait {
    pub fn new(started: Duration, limit: Duration, interval: Duration) -> Self {
        Self {
            fresh: FreshWait::new(started, limit, interval),
            retry_started: None,
        }
    }
    pub fn before_attempt(&mut self, now: &mut impl FnMut() -> Duration) {
        if self.retry_started.is_none() {
            self.retry_started = Some(now());
        }
    }
    pub fn finish(
        &mut self,
        result: Result<usize, crate::stream_ring::ReadError>,
        now: &mut impl FnMut() -> Duration,
    ) -> WaitStep<crate::stream_ring::ReadError> {
        let result = match result {
            Err(crate::stream_ring::ReadError::Retry) => {
                // Preserve the original short-circuit order and separate clock observations.
                if now().saturating_sub(self.retry_started.expect("attempt started"))
                    < Duration::from_millis(1)
                    && now().saturating_sub(self.fresh.started) < self.fresh.limit
                {
                    return WaitStep::RetryNow;
                }
                Ok(0)
            }
            other => other,
        };
        let result = self.fresh.finish(result, now);
        if matches!(result, WaitStep::SleepUntil(_)) {
            self.retry_started = None;
        }
        result
    }
}

pub struct SnapshotWait {
    deadline: Duration,
    interval: Duration,
}
impl SnapshotWait {
    pub fn new(deadline: Duration, interval: Duration) -> Self {
        assert!(!interval.is_zero());
        Self { deadline, interval }
    }
    pub fn before_attempt(&self, now: Duration) -> bool {
        now < self.deadline
    }
    pub fn retry_at(&self, now: Duration) -> Duration {
        now + self.interval
    }
}

pub struct BufferedWait {
    snapshot: SnapshotWait,
    freshness: Duration,
    last_progress: Duration,
    observed: Option<(u64, u64)>,
}
impl BufferedWait {
    pub fn new(
        started: Duration,
        deadline: Duration,
        freshness: Duration,
        interval: Duration,
    ) -> Self {
        assert!(!freshness.is_zero());
        Self {
            snapshot: SnapshotWait::new(deadline, interval),
            freshness,
            last_progress: started,
            observed: None,
        }
    }
    pub fn before_attempt(&self, now: Duration) -> bool {
        self.snapshot.before_attempt(now) && now.saturating_sub(self.last_progress) < self.freshness
    }
    pub fn observe_progress(&mut self, progress: (u64, u64), now: &mut impl FnMut() -> Duration) {
        if let Some((generation, written)) = self.observed {
            if (progress.0 == generation && progress.1 > written)
                || (progress.0 != generation && progress.1 > 0)
            {
                self.last_progress = now();
            }
        }
        self.observed = Some(progress);
    }
    pub fn retained<T, E>(
        &self,
        result: Result<Option<T>, E>,
    ) -> Result<Option<T>, BufferWaitError> {
        result.map_err(|_| BufferWaitError::Unsupported)
    }
    pub fn retry_at(&self, now: Duration) -> Duration {
        self.snapshot.retry_at(now)
    }
}

/// Reads fresh data within `limit`, releasing the producer's lock before sleeping.
/// Any nonzero read is progress, including silent samples. A read error stays distinct from a
/// stalled producer. A retry may run one `interval` past the limit, plus scheduler delay.
pub fn read_fresh<E>(
    limit: Duration,
    interval: Duration,
    mut read: impl FnMut() -> Result<usize, E>,
) -> Result<usize, WaitError<E>> {
    let origin = Instant::now();
    let wait = FreshWait::new(Duration::ZERO, limit, interval);
    loop {
        match wait.finish(read(), &mut || origin.elapsed()) {
            WaitStep::Ready(n) => return Ok(n),
            WaitStep::Failed(error) => return Err(WaitError::Read(error)),
            WaitStep::Stalled => return Err(WaitError::Stalled),
            WaitStep::SleepUntil(_) => std::thread::sleep(interval),
            WaitStep::RetryNow => unreachable!(),
        }
    }
}

/// Reads raw history while allowing callers to time each single copy.
/// Retry means a publication raced the copy, not that capture has no audio.
/// Retry attempts share a 1 ms wall budget; a copy or preemption can finish later.
/// Exhausted retries and empty reads retain the ordinary sleep policy.
pub fn read_atomic_fresh_with(
    limit: Duration,
    interval: Duration,
    mut read: impl FnMut() -> Result<usize, crate::stream_ring::ReadError>,
) -> Result<usize, WaitError<crate::stream_ring::ReadError>> {
    let origin = Instant::now();
    let mut wait = RawWait::new(Duration::ZERO, limit, interval);
    loop {
        wait.before_attempt(&mut || origin.elapsed());
        match wait.finish(read(), &mut || origin.elapsed()) {
            WaitStep::Ready(n) => return Ok(n),
            WaitStep::Failed(error) => return Err(WaitError::Read(error)),
            WaitStep::Stalled => return Err(WaitError::Stalled),
            WaitStep::SleepUntil(_) => std::thread::sleep(interval),
            WaitStep::RetryNow => std::hint::spin_loop(),
        }
    }
}

/// Copies fresh raw audio within one wait budget, sleeping between retries.
pub fn read_atomic_fresh(
    ring: &crate::stream_ring::StreamRing,
    cursor: &mut crate::stream_ring::Cursor,
    out: &mut [i32],
    limit: Duration,
    interval: Duration,
) -> Result<usize, WaitError<crate::stream_ring::ReadError>> {
    read_atomic_fresh_with(limit, interval, || {
        ring.read(cursor, out).map(|read| read.frames)
    })
}

/// Takes bounded header snapshots until an absolute deadline.
pub fn cursor_before(
    ring: &crate::stream_ring::StreamRing,
    deadline: Instant,
    interval: Duration,
    mut cancelled: impl FnMut() -> bool,
) -> Option<crate::stream_ring::Cursor> {
    assert!(!interval.is_zero());
    let origin = Instant::now();
    let wait = SnapshotWait::new(deadline.saturating_duration_since(origin), interval);
    while wait.before_attempt(origin.elapsed()) {
        if cancelled() {
            return None;
        }
        match ring.cursor_now() {
            Ok(cursor) => return Some(cursor),
            Err(crate::stream_ring::ReadError::Retry) => std::thread::sleep(interval),
            Err(crate::stream_ring::ReadError::Overrun) => return None,
        }
    }
    None
}

/// Reads a shared ring within one fresh-data budget, including contention on its mutex.
pub fn read_locked_fresh<T, E>(
    ring: &Mutex<T>,
    limit: Duration,
    interval: Duration,
    mut read: impl FnMut(&T) -> Result<usize, E>,
) -> Result<usize, WaitError<E>> {
    read_fresh(limit, interval, || match ring.try_lock() {
        Ok(ring) => read(&ring),
        Err(TryLockError::WouldBlock) => Ok(0),
        Err(TryLockError::Poisoned(_)) => panic!("stream ring lock poisoned"),
    })
}

/// Acquires a shared ring before one absolute deadline, for cursor/capacity snapshots.
pub fn lock_before<T>(
    ring: &Mutex<T>,
    deadline: Instant,
    interval: Duration,
) -> Option<MutexGuard<'_, T>> {
    assert!(!interval.is_zero());
    let origin = Instant::now();
    let wait = SnapshotWait::new(deadline.saturating_duration_since(origin), interval);
    loop {
        if !wait.before_attempt(origin.elapsed()) {
            return None;
        }
        match ring.try_lock() {
            Ok(ring) => return Some(ring),
            Err(TryLockError::WouldBlock) => std::thread::sleep(interval),
            Err(TryLockError::Poisoned(_)) => panic!("stream ring lock poisoned"),
        }
    }
}

/// Why retained-history admission failed before sending a successful response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferWaitError {
    Stalled,
    Disconnected,
    Unsupported,
}

/// Selects retained history under its ring lock, with independent fill and freshness budgets.
/// Resets may refill history but cannot restart the absolute deadline. Socket work stays
/// outside the ring lock, and contention consumes the same bounded wait.
pub fn buffered_cursor_before(
    ring: &Mutex<crate::flac_ring::FlacRing>,
    sock: &std::net::TcpStream,
    frames: usize,
    deadline: Instant,
    freshness: Duration,
    interval: Duration,
) -> Result<crate::flac_ring::Cursor, BufferWaitError> {
    assert!(!freshness.is_zero() && !interval.is_zero());
    let origin = Instant::now();
    let mut wait = BufferedWait::new(
        Duration::ZERO,
        deadline.saturating_duration_since(origin),
        freshness,
        interval,
    );
    loop {
        if !wait.before_attempt(origin.elapsed()) {
            return Err(BufferWaitError::Stalled);
        }
        if !crate::stream_http::peer_connected(sock).unwrap_or(false) {
            return Err(BufferWaitError::Disconnected);
        }
        match ring.try_lock() {
            Ok(ring) => {
                wait.observe_progress(ring.progress(), &mut || origin.elapsed());
                match wait.retained(ring.cursor_retained(frames)) {
                    Ok(Some(cursor)) => return Ok(cursor),
                    Ok(None) => {}
                    Err(_) => return Err(BufferWaitError::Unsupported),
                }
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Poisoned(_)) => panic!("stream ring lock poisoned"),
        }
        std::thread::sleep(interval);
    }
}

/// Two payload owners, including sockets whose checked close has not completed.
#[derive(Default)]
pub struct Capacity(CapacityMutex<Registry>);
#[derive(Default)]
struct Registry {
    owners: [Owner; 2],
    intent: Option<Intent>,
    next_id: u64,
}
#[derive(Default, Clone, Copy)]
struct Owner {
    id: u64,
    intent: u64,
    revoked: bool,
}
struct Intent {
    id: u64,
    peer: std::net::IpAddr,
    network: crate::network_lifecycle::Network,
    lease: crate::network_lifecycle::Lease,
    token: Arc<CapacityMutex<bool>>,
    deadline: Instant,
    format: crate::stream_buffer::OutputFormat,
    buffer: Option<crate::stream_buffer::BufferPreset>,
    active: bool,
}
impl Registry {
    fn next(&mut self) -> Option<u64> {
        self.next_id = self.next_id.checked_add(1)?;
        Some(self.next_id)
    }
    fn claim(
        &mut self,
        id: u64,
        peer: std::net::IpAddr,
        generation: u32,
        format: crate::stream_buffer::OutputFormat,
        buffer: Option<crate::stream_buffer::BufferPreset>,
        now: Instant,
    ) -> Option<(usize, u64)> {
        let intent = self.intent.as_ref()?;
        if intent.id != id
            || intent.peer != peer
            || intent.lease.generation != generation
            || intent.format != format
            || intent.buffer != buffer
            || (!intent.active && now >= intent.deadline)
        {
            return None;
        }
        if let Some(owner) = self
            .owners
            .iter_mut()
            .find(|owner| owner.id != 0 && owner.intent == id)
        {
            owner.revoked = true;
            return None;
        }
        let index = self.owners.iter().position(|owner| owner.id == 0)?;
        let owner_id = self.next()?;
        self.owners[index] = Owner {
            id: owner_id,
            intent: id,
            revoked: false,
        };
        self.intent.as_mut().unwrap().active = true;
        Some((index, owner_id))
    }
    fn clear(&mut self, id: u64) {
        if self.intent.as_ref().is_some_and(|intent| intent.id == id) {
            self.intent = None;
            for owner in &mut self.owners {
                if owner.intent == id {
                    owner.revoked = true;
                }
            }
        }
    }
}
impl Capacity {
    /// Snapshot identity outside the registry lock to preserve token-before-capacity order.
    fn refresh(&self) {
        let snapshot = self.0.lock().unwrap().intent.as_ref().map(|intent| {
            (
                intent.id,
                intent.token.clone(),
                intent.network.clone(),
                intent.lease,
                intent.deadline,
                intent.active,
            )
        });
        if let Some((id, token, network, lease, deadline, active)) = snapshot {
            let permit = token.lock().unwrap();
            if !*permit || !network.is_current(lease) {
                self.clear(id);
            } else if !active && Instant::now() >= deadline {
                let mut registry = self.0.lock().unwrap();
                if registry.intent.as_ref().is_some_and(|intent| {
                    intent.id == id && !intent.active && Instant::now() >= intent.deadline
                }) {
                    registry.clear(id);
                }
            }
        }
    }
    pub fn held(&self) -> usize {
        self.0
            .lock()
            .unwrap()
            .owners
            .iter()
            .filter(|owner| owner.id != 0)
            .count()
    }
    /// Installs one absolute pending lifetime; later checks never extend it.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve(
        &self,
        peer: std::net::IpAddr,
        network: crate::network_lifecycle::Network,
        lease: crate::network_lifecycle::Lease,
        token: Arc<CapacityMutex<bool>>,
        format: crate::stream_buffer::OutputFormat,
        buffer: Option<crate::stream_buffer::BufferPreset>,
        now: Instant,
    ) -> Option<u64> {
        let permit = token.lock().unwrap();
        if !*permit || !network.is_current(lease) {
            return None;
        }
        let mut registry = self.0.lock().unwrap();
        let id = registry.next()?;
        if let Some(old) = registry.intent.as_ref().map(|intent| intent.id) {
            registry.clear(old);
        }
        if registry
            .owners
            .iter()
            .filter(|owner| owner.id != 0 && owner.intent == 0)
            .count()
            == 2
        {
            let newest = registry
                .owners
                .iter_mut()
                .max_by_key(|owner| owner.id)
                .unwrap();
            newest.revoked = true;
        }
        registry.intent = Some(Intent {
            id,
            peer,
            network,
            lease,
            token: token.clone(),
            deadline: now + Duration::from_secs(60),
            format,
            buffer,
            active: false,
        });
        Some(id)
    }
    pub fn clear(&self, id: u64) {
        self.0.lock().unwrap().clear(id);
    }
    pub fn valid(&self, id: u64) -> bool {
        self.refresh();
        self.0
            .lock()
            .unwrap()
            .intent
            .as_ref()
            .is_some_and(|intent| intent.id == id)
    }
    /// Lifetime check for a caller already holding the associated command token gate.
    pub fn current(&self, id: u64) -> bool {
        let registry = self.0.lock().unwrap();
        registry.intent.as_ref().is_some_and(|intent| {
            intent.id == id && (intent.active || Instant::now() < intent.deadline)
        })
    }
    pub fn current_ready(&self, id: u64) -> bool {
        let registry = self.0.lock().unwrap();
        registry.intent.as_ref().is_some_and(|intent| {
            intent.id == id && (intent.active || Instant::now() < intent.deadline)
        }) && registry.owners.iter().any(|owner| owner.id == 0)
    }
    pub fn ready(&self, id: u64) -> bool {
        self.refresh();
        let registry = self.0.lock().unwrap();
        registry
            .intent
            .as_ref()
            .is_some_and(|intent| intent.id == id)
            && registry.owners.iter().any(|owner| owner.id == 0)
    }
    /// Classifies the canonical tagged media route before any reconnect wait or allocation.
    /// A malformed tag never reaches here: `StreamRequest::parse` refuses it 400 first.
    pub fn admit_request(
        capacity: &Arc<Self>,
        request: &crate::stream_http::StreamRequest,
        peer: std::net::IpAddr,
        generation: u32,
    ) -> Result<Option<Slot>, crate::stream_http::Status> {
        use crate::stream_http::{Route, Status};
        capacity.refresh();
        let id = request.sonos_intent.ok_or(Status::ServiceUnavailable)?;
        let (format, buffer) = {
            let registry = capacity.0.lock().unwrap();
            let intent = registry.intent.as_ref().ok_or(Status::ServiceUnavailable)?;
            if intent.id != id
                || intent.peer != peer
                || request.generation != Some(generation)
                || intent.lease.generation != generation
            {
                return Err(Status::ServiceUnavailable);
            }
            (intent.format, intent.buffer)
        };
        let expected = buffer.map_or(Route::Stream(format.format()), Route::Buffered);
        let route = request.route(format);
        if route == Route::Refuse(Status::BadRequest) {
            return Err(Status::BadRequest);
        }
        if route != expected {
            return Err(Status::ServiceUnavailable);
        }
        Ok(Self::admit(capacity, id, peer, generation, format, buffer))
    }
    pub fn admit(
        capacity: &Arc<Self>,
        id: u64,
        peer: std::net::IpAddr,
        generation: u32,
        format: crate::stream_buffer::OutputFormat,
        buffer: Option<crate::stream_buffer::BufferPreset>,
    ) -> Option<Slot> {
        capacity.refresh();
        // Hold the original cancellation gate through the claim, never acquire it under registry.
        let token = capacity.0.lock().unwrap().intent.as_ref()?.token.clone();
        let permit = token.lock().unwrap();
        if !*permit {
            return None;
        }
        let mut registry = capacity.0.lock().unwrap();
        let (index, owner_id) =
            registry.claim(id, peer, generation, format, buffer, Instant::now())?;
        Some(Slot {
            capacity: Arc::clone(capacity),
            index: index as u32,
            owner_id,
        })
    }
}
/// One listener place. Drop the socket before dropping this guard.
#[repr(C)]
pub struct Slot {
    capacity: Arc<Capacity>,
    index: u32,
    owner_id: u64,
}
impl Slot {
    pub fn take(capacity: &Arc<Capacity>, limit: usize) -> Option<Self> {
        capacity.refresh();
        let mut registry = capacity.0.lock().unwrap();
        let held = registry.owners.iter().filter(|owner| owner.id != 0).count();
        if held >= limit.min(2)
            || (registry.intent.is_some()
                && registry
                    .owners
                    .iter()
                    .any(|owner| owner.id != 0 && owner.intent == 0))
        {
            return None;
        }
        let index = registry.owners.iter().position(|owner| owner.id == 0)?;
        let owner_id = registry.next()?;
        registry.owners[index] = Owner {
            id: owner_id,
            intent: 0,
            revoked: false,
        };
        Some(Self {
            capacity: Arc::clone(capacity),
            index: index as u32,
            owner_id,
        })
    }
    pub fn owner_id(&self) -> u64 {
        self.owner_id
    }
    pub fn intent_id(&self) -> u64 {
        let registry = self.capacity.0.lock().unwrap();
        let owner = registry.owners[self.index as usize];
        if owner.id == self.owner_id {
            owner.intent
        } else {
            0
        }
    }
    pub fn taken(&self) -> usize {
        self.capacity.held()
    }
    pub fn revoked(&self) -> bool {
        self.capacity.refresh();
        let registry = self.capacity.0.lock().unwrap();
        let owner = &registry.owners[self.index as usize];
        owner.id != self.owner_id || owner.revoked
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        let mut registry = self.capacity.0.lock().unwrap();
        let owner = &mut registry.owners[self.index as usize];
        if owner.id == self.owner_id {
            *owner = Owner::default();
        }
    }
}
/// Object sizes and the Arc allocation layout, excluding native mutex backing/allocator overhead.
/// ArcInner is repr(C, align(2)) with strong/weak usize atomics in the pinned Rust source.
pub fn capacity_metadata_layout() -> [usize; 7] {
    #[repr(C, align(2))]
    struct ArcAllocation {
        strong: std::sync::atomic::AtomicUsize,
        weak: std::sync::atomic::AtomicUsize,
        data: Capacity,
    }
    [
        std::mem::size_of::<Slot>(),
        std::mem::size_of::<Owner>(),
        std::mem::size_of::<Intent>(),
        std::mem::size_of::<Registry>(),
        std::mem::size_of::<Capacity>(),
        std::mem::offset_of!(ArcAllocation, data),
        std::mem::size_of::<ArcAllocation>(),
    ]
}
#[cfg(target_os = "espidf")]
const _: () = {
    assert!(std::mem::size_of::<Slot>() <= 16);
    assert!(std::mem::size_of::<Owner>() <= 24);
    assert!(std::mem::size_of::<Intent>() <= 80);
    assert!(std::mem::size_of::<Registry>() <= 160);
};

#[cfg(all(test, not(feature = "listener-loom")))]
mod tests {
    use super::*;
    use crate::capture_buffer::CaptureBuffer;
    use crate::flac_ring::FlacRing;
    use crate::stream_http::{DeadlineWriter, Format};
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::{mpsc, Barrier, Mutex};
    use std::thread;
    use std::time::Instant;

    const LIMIT: Duration = Duration::from_millis(50);
    const INTERVAL: Duration = Duration::from_millis(1);

    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (server, client)
    }

    #[test]
    fn stalled_raw_and_encoded_producers_release_both_listener_slots() {
        let slots = Arc::new(Capacity::default());
        let (_, raw) = CaptureBuffer::new(8, 1);
        let encoded = Arc::new(Mutex::new(FlacRing::try_new(2, 16).unwrap()));
        let (finished, results) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        let begin_wait = Arc::new(Barrier::new(3));
        let mut clients = Vec::new();
        for format in [Format::Flac, Format::Wav] {
            let slot = Slot::take(&slots, 2).unwrap();
            let raw = raw.clone();
            let encoded = encoded.clone();
            let finished = finished.clone();
            let started = started.clone();
            let begin_wait = begin_wait.clone();
            let (server, client) = pair();
            clients.push(client);
            thread::spawn(move || {
                DeadlineWriter::new(&server)
                    .write_all_before(
                        format.response_head().as_bytes(),
                        Instant::now() + Duration::from_secs(1),
                    )
                    .unwrap();
                started.send(()).unwrap();
                begin_wait.wait();
                let stalled = match format {
                    Format::Wav => {
                        let mut cursor = raw.cursor_now().unwrap();
                        let mut out = [0; 2];
                        matches!(
                            read_atomic_fresh(&raw, &mut cursor, &mut out, LIMIT, INTERVAL),
                            Err(WaitError::Stalled)
                        )
                    }
                    Format::Flac | Format::Mp3 => {
                        let mut cursor = encoded.lock().unwrap().cursor_now();
                        let mut out = [0; 16];
                        matches!(
                            read_locked_fresh(&encoded, LIMIT, INTERVAL, |ring| ring
                                .read(&mut cursor, &mut out)
                                .map(|read| read.bytes)),
                            Err(WaitError::Stalled)
                        )
                    }
                };
                drop(server);
                drop(slot);
                finished.send(stalled).unwrap();
            });
        }
        assert!(Slot::take(&slots, 2).is_none());
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
        // One peer disconnects; the other stays connected and observes the server's EOF.
        drop(clients.pop().unwrap());
        begin_wait.wait();
        for _ in 0..2 {
            assert!(results
                .recv_timeout(Duration::from_millis(300))
                .expect("a stalled producer must release its listener"));
        }
        assert_eq!(slots.held(), 0);
        assert!(Slot::take(&slots, 2).is_some());
        let mut response = Vec::new();
        clients[0]
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        clients[0].read_to_end(&mut response).unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200"));
    }

    #[test]
    fn quiet_audio_after_a_short_capture_delay_is_fresh_data() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 1);
        let mut cursor = ring.cursor_now().unwrap();
        let thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            publisher.publish(&[0, 0, 0, 0], 0, || 0).unwrap();
        });
        let mut out = [1; 4];
        assert_eq!(
            read_atomic_fresh(
                &ring,
                &mut cursor,
                &mut out,
                Duration::from_secs(1),
                INTERVAL
            ),
            Ok(1)
        );
        assert_eq!(&out[..2], &[0; 2]);
        thread.join().unwrap();
    }

    #[test]
    fn an_encoded_quiet_block_after_a_short_delay_is_fresh_data() {
        let ring = Arc::new(Mutex::new(
            FlacRing::try_new(2, crate::flac::MAX_FRAME_BYTES).unwrap(),
        ));
        let mut cursor = ring.lock().unwrap().cursor_now();
        let mut encoder = crate::flac::Encoder::new();
        encoder.push(&vec![0; crate::flac::CHANNELS * crate::flac::BLOCK_FRAMES]);
        let mut frame = Vec::new();
        encoder.encode_block(0, &mut frame);
        let expected = frame.clone();
        let producer = ring.clone();
        let thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            producer
                .lock()
                .unwrap()
                .push(
                    &frame,
                    crate::flac_ring::SourceMetadata {
                        raw_generation: 0,
                        source_start: 0,
                        source_end: 1,
                        oldest_accepted_us: 0,
                        raw_publication_us: 0,
                    },
                    || 0,
                )
                .unwrap();
        });
        let mut out = vec![0; crate::flac::MAX_FRAME_BYTES];
        assert_eq!(
            read_locked_fresh(&ring, Duration::from_secs(1), INTERVAL, |ring| ring
                .read(&mut cursor, &mut out)
                .map(|read| read.bytes)),
            Ok(expected.len())
        );
        assert_eq!(&out[..expected.len()], &expected);
        thread.join().unwrap();
    }

    #[test]
    fn raw_and_encoded_resets_remain_read_errors() {
        let (mut publisher, raw) = CaptureBuffer::new(8, 1);
        let mut cursor = raw.cursor_now().unwrap();
        publisher.invalidate().unwrap();
        assert_eq!(
            read_atomic_fresh(&raw, &mut cursor, &mut [0; 2], LIMIT, INTERVAL),
            Err(WaitError::Read(crate::stream_ring::ReadError::Overrun))
        );
        let mut encoded = FlacRing::try_new(2, 16).unwrap();
        let mut cursor = encoded.cursor_now();
        encoded.reset().unwrap();
        assert_eq!(
            read_fresh(LIMIT, INTERVAL, || encoded
                .read(&mut cursor, &mut [0; 16])
                .map(|read| read.bytes)),
            Err(WaitError::Read(
                crate::flac_ring::ReadError::GenerationChanged
            ))
        );
    }

    #[test]
    fn a_shared_encoder_retry_gets_a_full_fresh_wait() {
        let (_, raw) = CaptureBuffer::new(8, 1);
        let mut cursor = raw.cursor_now().unwrap();
        let mut out = [0; 2];
        for _ in 0..2 {
            let started = Instant::now();
            assert_eq!(
                read_atomic_fresh(&raw, &mut cursor, &mut out, LIMIT, INTERVAL),
                Err(WaitError::Stalled)
            );
            assert!(started.elapsed() >= LIMIT);
        }
    }

    fn held_ring_timeout<T: Send + 'static>(
        ring: T,
        operation: impl FnOnce(Arc<Mutex<T>>) -> bool + Send + 'static,
    ) {
        let ring = Arc::new(Mutex::new(ring));
        let (locked, ready) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let owner_ring = ring.clone();
        let owner = thread::spawn(move || {
            let _guard = owner_ring.lock().unwrap();
            locked.send(()).unwrap();
            released.recv().unwrap();
        });
        ready.recv().unwrap();
        let (finished, result) = mpsc::channel();
        let reader = thread::spawn(move || finished.send(operation(ring)).unwrap());
        let timed_out = result.recv_timeout(Duration::from_millis(150));
        // Release/join before any assertion: a broken blocking implementation must not hang tests.
        release.send(()).unwrap();
        owner.join().unwrap();
        reader.join().unwrap();
        assert_eq!(timed_out, Ok(true), "held ring exceeded its 50 ms budget");
    }

    #[test]
    fn a_held_encoded_ring_cannot_block_the_fresh_deadline() {
        let mut encoded = FlacRing::try_new(2, 16).unwrap();
        let mut cursor = encoded.cursor_now();
        encoded
            .push(
                &[1, 2, 3],
                crate::flac_ring::SourceMetadata {
                    raw_generation: 0,
                    source_start: 0,
                    source_end: 1,
                    oldest_accepted_us: 0,
                    raw_publication_us: 0,
                },
                || 0,
            )
            .unwrap();
        held_ring_timeout(encoded, move |ring| {
            read_locked_fresh(&ring, LIMIT, INTERVAL, |encoded| {
                encoded
                    .read(&mut cursor, &mut [0; 16])
                    .map(|read| read.bytes)
            }) == Err(WaitError::Stalled)
        });
    }

    #[test]
    fn initial_cursor_lock_cannot_outlive_its_absolute_deadline() {
        held_ring_timeout(FlacRing::try_new(2, 16).unwrap(), |ring| {
            lock_before(&ring, Instant::now() + LIMIT, INTERVAL).is_none()
        });
    }
    #[test]
    fn a_real_ring_retry_revalidates_without_sleeping_a_full_interval() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 2);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[17, -17, 23, -23], 0, || 0).unwrap();
        let before = cursor;
        let mut out = [0; 4];
        let mut attempts = 0;
        let interval = Duration::from_millis(200);
        let started = Instant::now();
        let result = read_atomic_fresh_with(Duration::from_secs(1), interval, || {
            attempts += 1;
            if attempts == 1 {
                let result = ring
                    .read_with_copy_hook(&mut cursor, &mut out, || {
                        // Deterministic protocol fixture: cross an era during the real copy,
                        // keeping its published slot, head and generation unchanged.
                        // Ordinary publication into another retained slot need not reject a copy.
                        ring.commit((2, 1), (2, 0), 1, None, None, || 0);
                    })
                    .map(|read| read.frames);
                assert_eq!(result, Err(crate::stream_ring::ReadError::Retry));
                assert_eq!(
                    cursor, before,
                    "rejected scratch must not advance the cursor"
                );
                out.fill(i32::MIN);
                result
            } else {
                ring.read(&mut cursor, &mut out).map(|read| read.frames)
            }
        });
        assert_eq!(result, Ok(2));
        assert_eq!(attempts, 2);
        assert_eq!(out, [17, -17, 23, -23]);
        assert_ne!(cursor, before);
        assert!(
            started.elapsed() < interval / 2,
            "Retry slept a whole interval"
        );
    }

    #[test]
    fn raw_retry_wait_policy_yields_between_bounded_bursts_without_refreshing_budget() {
        // Callback policy fixture, not evidence about target publication or scheduling.
        let interval = Duration::from_millis(5);
        let limit = Duration::from_millis(30);
        let started = Instant::now();
        let mut previous = started;
        let mut longest_gap = Duration::ZERO;
        let result = read_atomic_fresh_with(limit, interval, || {
            let now = Instant::now();
            longest_gap = longest_gap.max(now.duration_since(previous));
            previous = now;
            Err::<usize, _>(crate::stream_ring::ReadError::Retry)
        });
        assert_eq!(result, Err(WaitError::Stalled));
        assert!(started.elapsed() >= limit);
        assert!(started.elapsed() < limit + interval + Duration::from_millis(100));
        assert!(
            longest_gap >= interval,
            "persistent Retry must yield between bursts"
        );
    }

    #[test]
    fn a_real_empty_ring_still_sleeps_before_retrying() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 2);
        let mut cursor = ring.cursor_now().unwrap();
        let mut out = [0; 4];
        let interval = Duration::from_millis(30);
        let started = Instant::now();
        let mut attempts = 0;
        let result = read_atomic_fresh_with(Duration::from_secs(1), interval, || {
            attempts += 1;
            let result = ring.read(&mut cursor, &mut out).map(|read| read.frames);
            if attempts == 1 {
                assert_eq!(result, Ok(0));
                publisher.publish(&[31, -31, 37, -37], 0, || 0).unwrap();
            }
            result
        });
        assert_eq!(result, Ok(2));
        assert_eq!(attempts, 2);
        assert_eq!(out, [31, -31, 37, -37]);
        assert!(
            started.elapsed() >= interval,
            "empty reads must retain the sleep policy"
        );
    }
}

#[cfg(all(test, not(feature = "listener-loom")))]
mod capacity_tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    fn parse(line: &str) -> crate::stream_http::StreamRequest<'_> {
        crate::stream_http::StreamRequest::parse(line).unwrap()
    }
    fn setup() -> (
        Arc<Capacity>,
        crate::network_lifecycle::Network,
        crate::network_lifecycle::Lease,
        std::sync::Arc<std::sync::Mutex<bool>>,
    ) {
        let network = crate::network_lifecycle::Network::new();
        network.observe(network.snapshot(), Some(Ipv4Addr::new(192, 0, 2, 1)));
        let lease = network.lease().unwrap();
        (
            Arc::new(Capacity::default()),
            network,
            lease,
            std::sync::Arc::new(std::sync::Mutex::new(true)),
        )
    }
    #[test]
    fn delayed_pending_refresh_cannot_expire_an_owner_activated_while_it_waited() {
        let (capacity, network, lease, token) = setup();
        let peer = Ipv4Addr::new(192, 0, 2, 2).into();
        let id = capacity
            .reserve(
                peer,
                network,
                lease,
                token.clone(),
                crate::stream_buffer::OutputFormat::Flac,
                None,
                Instant::now(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        capacity.0.lock().unwrap().intent.as_mut().unwrap().deadline = deadline;
        let permit = token.lock().unwrap();
        let before = std::sync::Arc::strong_count(&token);
        let checking = capacity.clone();
        let reader = std::thread::spawn(move || checking.valid(id));
        // The snapshot owns a token clone before waiting on the actual token mutex.
        while std::sync::Arc::strong_count(&token) == before {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let (index, owner_id) = capacity
            .0
            .lock()
            .unwrap()
            .claim(
                id,
                peer,
                lease.generation,
                crate::stream_buffer::OutputFormat::Flac,
                None,
                Instant::now(),
            )
            .unwrap();
        let owner = Slot {
            capacity: capacity.clone(),
            index: index as u32,
            owner_id,
        };
        std::thread::sleep(
            deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
        );
        drop(permit);
        assert!(
            reader.join().unwrap(),
            "stale pending snapshot revoked an active reservation"
        );
        assert!(!owner.revoked());
        drop(owner);
    }

    #[test]
    fn pending_checks_never_renew_expiry_but_active_reconnect_keeps_protection() {
        let (capacity, network, lease, token) = setup();
        let peer = Ipv4Addr::new(192, 0, 2, 2).into();
        let installed = Instant::now();
        let id = capacity
            .reserve(
                peer,
                network,
                lease,
                token.clone(),
                crate::stream_buffer::OutputFormat::Flac,
                None,
                installed,
            )
            .unwrap();
        for _ in 0..4 {
            assert!(capacity.ready(id));
            assert!(capacity.valid(id));
            assert!(capacity.current_ready(id));
            assert_eq!(
                capacity.0.lock().unwrap().intent.as_ref().unwrap().deadline,
                installed + Duration::from_secs(60)
            );
        }
        capacity.0.lock().unwrap().intent.as_mut().unwrap().deadline =
            Instant::now() - Duration::from_secs(1);
        assert!(!capacity.ready(id));
        assert!(!capacity.valid(id));
        let (capacity, network, lease, token) = setup();
        let id = capacity
            .reserve(
                peer,
                network,
                lease,
                token.clone(),
                crate::stream_buffer::OutputFormat::Flac,
                None,
                Instant::now(),
            )
            .unwrap();
        let owner = Capacity::admit(
            &capacity,
            id,
            peer,
            lease.generation,
            crate::stream_buffer::OutputFormat::Flac,
            None,
        )
        .unwrap();
        capacity.0.lock().unwrap().intent.as_mut().unwrap().deadline =
            Instant::now() - Duration::from_secs(1);
        assert!(capacity.valid(id));
        drop(owner);
        let owner = Capacity::admit(
            &capacity,
            id,
            peer,
            lease.generation,
            crate::stream_buffer::OutputFormat::Flac,
            None,
        )
        .unwrap();
        *token.lock().unwrap() = false;
        assert!(owner.revoked());
        assert_eq!(capacity.held(), 1);
        drop(owner);
        assert_eq!(capacity.held(), 0);
    }

    #[test]
    fn initial_cursor_admission_checks_cancellation_before_reading() {
        let (_, ring) = crate::capture_buffer::CaptureBuffer::new(8, 1);
        assert!(cursor_before(
            &ring,
            Instant::now() + Duration::from_secs(1),
            Duration::from_millis(1),
            || true
        )
        .is_none());
    }
    #[test]
    fn capacity_layout_reports_concrete_objects_and_arc_allocation() {
        let layout = capacity_metadata_layout();
        assert_eq!(layout[0], std::mem::size_of::<Slot>());
        assert!(layout[1] <= 24);
        assert_eq!(layout[5], 2 * std::mem::size_of::<usize>());
        assert!(layout[6] >= layout[3] + layout[5]);
        eprintln!("capacity layout: slot={} owner={} intent={} registry={} mutex_registry={} arc_data_offset={} arc_allocation={}",layout[0],layout[1],layout[2],layout[3],layout[4],layout[5],layout[6]);
    }
    #[test]
    fn pending_reservation_revokes_newest_and_keeps_draining_place_occupied() {
        let (capacity, network, lease, token) = setup();
        let older = Slot::take(&capacity, 2).unwrap();
        let newer = Slot::take(&capacity, 2).unwrap();
        assert!(Slot::take(&capacity, 2).is_none());
        let id = capacity
            .reserve(
                Ipv4Addr::new(192, 0, 2, 2).into(),
                network,
                lease,
                token,
                crate::stream_buffer::OutputFormat::Flac,
                None,
                Instant::now(),
            )
            .unwrap();
        assert!(!older.revoked());
        assert!(newer.revoked());
        assert_eq!(capacity.held(), 2);
        assert!(!capacity.ready(id));
        assert!(Slot::take(&capacity, 2).is_none());
        drop(newer);
        assert!(capacity.ready(id));
        assert!(Slot::take(&capacity, 2).is_none());
        let sonos = Capacity::admit(
            &capacity,
            id,
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)),
            lease.generation,
            crate::stream_buffer::OutputFormat::Flac,
            None,
        )
        .unwrap();
        assert!(Slot::take(&capacity, 2).is_none());
        assert!(Capacity::admit(
            &capacity,
            id,
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)),
            lease.generation,
            crate::stream_buffer::OutputFormat::Flac,
            None
        )
        .is_none());
        assert!(sonos.revoked());
        assert!(!older.revoked());
        drop(sonos);
        assert!(Capacity::admit(
            &capacity,
            id,
            Ipv4Addr::new(192, 0, 2, 2).into(),
            lease.generation,
            crate::stream_buffer::OutputFormat::Flac,
            None
        )
        .is_some());
    }
    #[test]
    fn tagged_wav_wrong_buffer_wrong_peer_and_stale_identity_are_refused() {
        let (capacity, network, lease, token) = setup();
        let peer = Ipv4Addr::new(192, 0, 2, 2).into();
        let id = capacity
            .reserve(
                peer,
                network,
                lease,
                token,
                crate::stream_buffer::OutputFormat::Flac,
                Some(crate::stream_buffer::BufferPreset::Medium),
                Instant::now(),
            )
            .unwrap();
        for query in [
            format!(
                "/stream.wav?sonos_intent={id}&network_generation={}",
                lease.generation
            ),
            format!(
                "/stream.flac?buffer=low&sonos_intent={id}&network_generation={}",
                lease.generation
            ),
            format!(
                "/stream.flac?buffer=med&sonos_intent={}&network_generation={}",
                id + 1,
                lease.generation
            ),
        ] {
            assert!(matches!(
                Capacity::admit_request(
                    &capacity,
                    &parse(&format!("GET {query} HTTP/1.1")),
                    peer,
                    lease.generation
                ),
                Err(crate::stream_http::Status::ServiceUnavailable)
            ));
        }
        let line = format!(
            "GET /stream.flac?buffer=med&sonos_intent={id}&network_generation={} HTTP/1.1",
            lease.generation
        );
        assert!(matches!(
            Capacity::admit_request(
                &capacity,
                &parse(&line),
                Ipv4Addr::LOCALHOST.into(),
                lease.generation
            ),
            Err(crate::stream_http::Status::ServiceUnavailable)
        ));
        assert!(
            Capacity::admit_request(&capacity, &parse(&line), peer, lease.generation)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn malformed_tags_are_bad_requests_before_the_registry_is_consulted() {
        use crate::stream_http::{Status, StreamRequest};
        let (capacity, network, lease, token) = setup();
        let peer = Ipv4Addr::new(192, 0, 2, 2).into();
        let id = capacity
            .reserve(
                peer,
                network,
                lease,
                token,
                crate::stream_buffer::OutputFormat::Flac,
                Some(crate::stream_buffer::BufferPreset::Medium),
                Instant::now(),
            )
            .unwrap();
        let generation = lease.generation;
        // The composition `answer` runs: parse once, then admit the parsed request.
        let admit = |line: &str| {
            StreamRequest::parse(line)
                .and_then(|request| Capacity::admit_request(&capacity, &request, peer, generation))
        };
        for query in [
            format!("buffer=med&sonos_intent=x&network_generation={generation}"),
            format!(
                "buffer=med&sonos_intent={id}&sonos_intent={id}&network_generation={generation}"
            ),
            format!("buffer=med&sonos_intent={id}&network_generation="),
            format!("buffer=med&sonos_intent={id}&network_generation={generation}&exact=2"),
            format!("buffer=huge&sonos_intent={id}&network_generation={generation}"),
            format!("buffer=med&sonos_intent={id}&network_generation={generation}&cache=1"),
        ] {
            assert!(
                matches!(
                    admit(&format!("GET /stream.flac?{query} HTTP/1.1")),
                    Err(Status::BadRequest)
                ),
                "{query}"
            );
            assert!(capacity.valid(id), "{query}");
            assert_eq!(capacity.held(), 0, "{query}");
        }
        assert!(admit(&format!(
            "GET /stream.flac?buffer=med&sonos_intent={id}&network_generation={generation} HTTP/1.1"
        ))
        .unwrap()
        .is_some());
    }
    #[test]
    fn identity_expiry_cancel_and_old_cleanup_cannot_grant_priority() {
        let (capacity, network, lease, token) = setup();
        let peer = Ipv4Addr::new(192, 0, 2, 2).into();
        let id = capacity
            .reserve(
                peer,
                network.clone(),
                lease,
                token.clone(),
                crate::stream_buffer::OutputFormat::Mp3,
                None,
                Instant::now(),
            )
            .unwrap();
        assert!(Capacity::admit(
            &capacity,
            id,
            Ipv4Addr::LOCALHOST.into(),
            lease.generation,
            crate::stream_buffer::OutputFormat::Mp3,
            None
        )
        .is_none());
        assert!(Capacity::admit(
            &capacity,
            id,
            peer,
            lease.generation + 1,
            crate::stream_buffer::OutputFormat::Mp3,
            None
        )
        .is_none());
        assert!(Capacity::admit(
            &capacity,
            id,
            peer,
            lease.generation,
            crate::stream_buffer::OutputFormat::Flac,
            None
        )
        .is_none());
        *token.lock().unwrap() = false;
        assert!(!capacity.valid(id));
        let next = capacity
            .reserve(
                peer,
                network.clone(),
                lease,
                std::sync::Arc::new(std::sync::Mutex::new(true)),
                crate::stream_buffer::OutputFormat::Mp3,
                None,
                Instant::now(),
            )
            .unwrap();
        capacity.clear(id);
        assert!(capacity.valid(next));
        network.invalidate();
        assert!(!capacity.valid(next));
        assert!(Slot::take(&capacity, 2).is_some());
        let (capacity, network, lease, token) = setup();
        let id = capacity
            .reserve(
                peer,
                network,
                lease,
                token,
                crate::stream_buffer::OutputFormat::Mp3,
                None,
                Instant::now() - Duration::from_secs(60),
            )
            .unwrap();
        assert!(!capacity.valid(id));
    }
}
