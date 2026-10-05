// ABOUTME: Waits for fresh stream data and holds listener slots until their sockets close.
// ABOUTME: Shares the firmware's producer-wait and slot-release behavior with real host tests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flac_ring::FlacRing;
    use crate::stream_http::{DeadlineWriter, Format};
    use crate::stream_ring::StreamRing;
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
        let raw = Arc::new(Mutex::new(StreamRing::new(8)));
        let encoded = Arc::new(Mutex::new(FlacRing::try_new(2, 16).unwrap()));
        let (finished, results) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        let begin_wait = Arc::new(Barrier::new(3));
        let mut clients = Vec::new();
        for format in [Format::Wav, Format::Flac] {
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
                        let mut cursor = raw.lock().unwrap().cursor_now();
                        let mut out = [0; 2];
                        matches!(
                            read_fresh(LIMIT, INTERVAL, || raw
                                .lock()
                                .unwrap()
                                .read(&mut cursor, &mut out)),
                            Err(WaitError::Stalled)
                        )
                    }
                    Format::Flac => {
                        let mut cursor = encoded.lock().unwrap().cursor_now();
                        let mut out = [0; 16];
                        matches!(
                            read_fresh(LIMIT, INTERVAL, || encoded
                                .lock()
                                .unwrap()
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
        let ring = Arc::new(Mutex::new(StreamRing::new(8)));
        let mut cursor = ring.lock().unwrap().cursor_now();
        let producer = ring.clone();
        let thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            producer.lock().unwrap().push(&[0, 0, 0, 0]);
        });
        let mut out = [1; 4];
        assert_eq!(
            read_fresh(Duration::from_secs(1), INTERVAL, || ring
                .lock()
                .unwrap()
                .read(&mut cursor, &mut out)),
            Ok(2)
        );
        assert_eq!(out, [0; 4]);
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
            read_fresh(Duration::from_secs(1), INTERVAL, || ring
                .lock()
                .unwrap()
                .read(&mut cursor, &mut out)),
            Ok(expected.len())
        );
        assert_eq!(&out[..expected.len()], &expected);
        thread.join().unwrap();
    }

    #[test]
    fn raw_and_encoded_resets_remain_read_errors() {
        let mut raw = StreamRing::new(8);
        let mut cursor = raw.cursor_now();
        raw.reset();
        assert_eq!(
            read_fresh(LIMIT, INTERVAL, || raw.read(&mut cursor, &mut [0; 2])),
            Err(WaitError::Read(crate::stream_ring::Overrun))
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
        let raw = StreamRing::new(8);
        let mut cursor = raw.cursor_now();
        let mut out = [0; 2];
        for _ in 0..2 {
            let started = Instant::now();
            assert_eq!(
                read_fresh(LIMIT, INTERVAL, || raw.read(&mut cursor, &mut out)),
                Err(WaitError::Stalled)
            );
            assert!(started.elapsed() >= LIMIT);
        }
    }
}
