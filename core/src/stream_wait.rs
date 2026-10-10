// ABOUTME: Waits for fresh stream data and holds listener slots until their sockets close.
// ABOUTME: Shares the firmware's producer-wait and slot-release behavior with real host tests.

#[cfg(all(feature = "listener-loom", target_os = "espidf"))]
compile_error!("listener-loom is host-only");
#[cfg(all(feature = "listener-loom", feature = "capture-loom"))]
compile_error!("listener-loom and capture-loom require separate model invocations");
#[cfg(feature = "listener-loom")]
use loom::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[cfg(not(feature = "listener-loom"))]
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum WaitError<E> {
    Stalled,
    Read(E),
}

/// Reads fresh data within `limit`, releasing the producer's lock before sleeping.
/// Any nonzero read is progress, including silent samples. A read error stays distinct from a
/// stalled producer. A retry may run one `interval` past the limit, plus scheduler delay.
pub fn read_fresh<E>(
    limit: Duration,
    interval: Duration,
    mut read: impl FnMut() -> Result<usize, E>,
) -> Result<usize, WaitError<E>> {
    assert!(!limit.is_zero() && !interval.is_zero());
    let started = Instant::now();
    loop {
        match read() {
            Ok(0) if started.elapsed() >= limit => return Err(WaitError::Stalled),
            Ok(0) => std::thread::sleep(interval),
            Ok(n) => return Ok(n),
            Err(error) => return Err(WaitError::Read(error)),
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
    const RETRY_SPIN_LIMIT: Duration = Duration::from_millis(1);
    let started = Instant::now();
    read_fresh(limit, interval, || {
        let retry_started = Instant::now();
        loop {
            match read() {
                Err(crate::stream_ring::ReadError::Retry)
                    if retry_started.elapsed() < RETRY_SPIN_LIMIT && started.elapsed() < limit =>
                {
                    std::hint::spin_loop();
                }
                // The outer wait sleeps one existing interval or ends its absolute budget.
                Err(crate::stream_ring::ReadError::Retry) => return Ok(0),
                result => return result,
            }
        }
    })
}

/// Copies fresh raw audio within one wait budget, sleeping between retries.
pub fn read_atomic_fresh(
    ring: &crate::stream_ring::StreamRing,
    cursor: &mut crate::stream_ring::Cursor,
    out: &mut [i32],
    limit: Duration,
    interval: Duration,
) -> Result<usize, WaitError<crate::stream_ring::ReadError>> {
    read_atomic_fresh_with(limit, interval, || ring.read(cursor, out))
}

/// Takes bounded header snapshots until an absolute deadline.
pub fn cursor_before(
    ring: &crate::stream_ring::StreamRing,
    deadline: Instant,
    interval: Duration,
) -> Option<crate::stream_ring::Cursor> {
    assert!(!interval.is_zero());
    while Instant::now() < deadline {
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
    loop {
        if Instant::now() >= deadline {
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
    let mut last_progress = Instant::now();
    let mut observed = None;
    loop {
        let now = Instant::now();
        if now >= deadline || now.duration_since(last_progress) >= freshness {
            return Err(BufferWaitError::Stalled);
        }
        if !crate::stream_http::peer_connected(sock).unwrap_or(false) {
            return Err(BufferWaitError::Disconnected);
        }
        match ring.try_lock() {
            Ok(ring) => {
                let progress = ring.progress();
                if let Some((generation, written)) = observed {
                    if (progress.0 == generation && progress.1 > written)
                        || (progress.0 != generation && progress.1 > 0)
                    {
                        last_progress = Instant::now();
                    }
                }
                observed = Some(progress);
                match ring.cursor_retained(frames) {
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

/// One listener place. Drop the socket before dropping this guard.
pub struct Slot(Arc<AtomicUsize>);

impl Slot {
    pub fn take(taken: &Arc<AtomicUsize>, limit: usize) -> Option<Self> {
        taken
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < limit).then_some(n + 1)
            })
            .ok()?;
        Some(Self(Arc::clone(taken)))
    }

    pub fn taken(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

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
        let slots = Arc::new(AtomicUsize::new(0));
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
                                .read(&mut cursor, &mut out)),
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
        assert_eq!(slots.load(Ordering::Acquire), 0);
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
            publisher.publish(&[0, 0, 0, 0]).unwrap();
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
            producer.lock().unwrap().push(&frame).unwrap();
        });
        let mut out = vec![0; crate::flac::MAX_FRAME_BYTES];
        assert_eq!(
            read_locked_fresh(&ring, Duration::from_secs(1), INTERVAL, |ring| ring
                .read(&mut cursor, &mut out)),
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
        encoded.reset();
        assert_eq!(
            read_fresh(LIMIT, INTERVAL, || encoded.read(&mut cursor, &mut [0; 16])),
            Err(WaitError::Read(crate::flac_ring::Overrun))
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
        encoded.push(&[1, 2, 3]).unwrap();
        held_ring_timeout(encoded, move |ring| {
            read_locked_fresh(&ring, LIMIT, INTERVAL, |encoded| {
                encoded.read(&mut cursor, &mut [0; 16])
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
        publisher.publish(&[17, -17, 23, -23]).unwrap();
        let before = cursor;
        let mut out = [0; 4];
        let mut attempts = 0;
        let interval = Duration::from_millis(200);
        let started = Instant::now();
        let result = read_atomic_fresh_with(Duration::from_secs(1), interval, || {
            attempts += 1;
            if attempts == 1 {
                let result = ring.read_with_copy_hook(&mut cursor, &mut out, || {
                    // Deterministic protocol fixture: cross an era during the real copy,
                    // keeping its published slot, head and generation unchanged.
                    // Ordinary publication into another retained slot need not reject a copy.
                    ring.commit(2, 1, 2, 0, 1, None);
                });
                assert_eq!(result, Err(crate::stream_ring::ReadError::Retry));
                assert_eq!(
                    cursor, before,
                    "rejected scratch must not advance the cursor"
                );
                out.fill(i32::MIN);
                result
            } else {
                ring.read(&mut cursor, &mut out)
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
            let result = ring.read(&mut cursor, &mut out);
            if attempts == 1 {
                assert_eq!(result, Ok(0));
                publisher.publish(&[31, -31, 37, -37]).unwrap();
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
