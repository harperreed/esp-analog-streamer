// ABOUTME: Exercises production blocking admission operations with real loopback peers.
// ABOUTME: Finite workers and a teardown owner close peers, wake accepts and join on failure.

#[path = "../../firmware/src/stream_admission.rs"]
mod stream_admission;

use esp_shazam_core::stream_http::{write_response, Status};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use stream_admission::{accept_one, read_head};

const HEAD: Duration = Duration::from_secs(2);
const REQUEST: &[u8] = b"GET /missing HTTP/1.1\r\n\r\n";

struct Fixture {
    address: SocketAddr,
    clients: Vec<TcpStream>,
    workers: Vec<JoinHandle<()>>,
    accepted: mpsc::Receiver<(SocketAddr, Instant)>,
}

impl Fixture {
    fn new(workers: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let listener = Arc::new(Mutex::new(listener));
        let (tx, accepted) = mpsc::channel();
        let workers = (0..workers)
            .map(|_| {
                let listener = listener.clone();
                let tx = tx.clone();
                thread::spawn(move || {
                    for _ in 0..2 {
                        let (socket, peer) = accept_one(&listener).unwrap();
                        let _peer_text = peer.to_string();
                        let started = Instant::now();
                        if tx.send((peer, started)).is_err() {
                            return;
                        }
                        if read_head(&socket, started + HEAD, 4096).is_ok() {
                            let _ = write_response(
                                &socket,
                                Status::NotFound.response().as_bytes(),
                                HEAD,
                            );
                        }
                    }
                })
            })
            .collect();
        Self {
            address,
            clients: Vec::new(),
            workers,
            accepted,
        }
    }

    fn connect(&mut self) -> TcpStream {
        let socket = TcpStream::connect(self.address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        socket.set_write_timeout(Some(HEAD)).unwrap();
        self.clients.push(socket.try_clone().unwrap());
        socket
    }

    fn accepted(&self) -> (SocketAddr, Instant) {
        self.accepted.recv_timeout(HEAD).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for socket in &self.clients {
            let _ = socket.shutdown(Shutdown::Both);
        }
        // At most two accept calls remain per worker. Wake every possible call with EOF.
        for _ in 0..self.workers.len() * 2 {
            if let Ok(socket) =
                TcpStream::connect_timeout(&self.address, Duration::from_millis(100))
            {
                let _ = socket.shutdown(Shutdown::Both);
            }
        }
        let mut panicked = false;
        for worker in self.workers.drain(..) {
            panicked |= worker.join().is_err();
        }
        if !thread::panicking() {
            assert!(!panicked, "fixture worker panicked");
        }
    }
}

fn response(socket: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    socket.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[test]
fn complete_second_request_progresses_before_first_head_deadline() {
    let mut fixture = Fixture::new(2);
    let mut slow = fixture.connect();
    slow.write_all(b"GET /missing HTTP/1.1\r\n").unwrap();
    let (_, first_started) = fixture.accepted();
    let mut complete = fixture.connect();
    complete.write_all(REQUEST).unwrap();
    let result = response(&mut complete);
    assert_eq!(result.unwrap(), Status::NotFound.response().as_bytes());
    assert!(Instant::now() < first_started + HEAD);
}

#[test]
fn two_slow_heads_delay_third_until_worker_available() {
    let mut fixture = Fixture::new(2);
    let mut first = fixture.connect();
    first.write_all(b"GET /missing HTTP/1.1\r\n").unwrap();
    fixture.accepted();
    let mut second = fixture.connect();
    second.write_all(b"GET /missing HTTP/1.1\r\n").unwrap();
    fixture.accepted();
    let connected = Instant::now();
    let mut third = fixture.connect();
    let third_peer = third.local_addr().unwrap();
    third.write_all(REQUEST).unwrap();
    assert!(fixture
        .accepted
        .recv_timeout(Duration::from_millis(100))
        .is_err());
    let mut byte = [0];
    third
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    assert!(matches!(
        third.read(&mut byte).unwrap_err().kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    ));
    first.shutdown(Shutdown::Both).unwrap();
    let (peer, accepted) = fixture.accepted();
    assert_eq!(peer, third_peer);
    third.set_read_timeout(Some(HEAD)).unwrap();
    assert_eq!(
        response(&mut third).unwrap(),
        Status::NotFound.response().as_bytes()
    );
    let finished = Instant::now();
    eprintln!(
        "third connect-to-response={:?}; accept-to-response={:?}",
        finished - connected,
        finished - accepted
    );
}

struct HeadPeer {
    client: TcpStream,
    worker: Option<JoinHandle<io::Result<String>>>,
    deadline: Instant,
}

impl HeadPeer {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.set_write_timeout(Some(HEAD)).unwrap();
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (socket, peer) = accept_one(&Mutex::new(listener)).unwrap();
            let _peer_text = peer.to_string();
            let deadline = Instant::now() + HEAD;
            tx.send(deadline).unwrap();
            read_head(&socket, deadline, 4096)
        });
        let mut peer = Self {
            client,
            worker: Some(worker),
            deadline: Instant::now(),
        };
        peer.deadline = rx.recv_timeout(HEAD).unwrap();
        peer
    }

    fn finish(mut self) -> io::Result<String> {
        self.worker.take().unwrap().join().unwrap()
    }
}

impl Drop for HeadPeer {
    fn drop(&mut self) {
        let _ = self.client.shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join().expect("head worker panicked");
        }
    }
}

fn assert_deadline(peer: HeadPeer) {
    let deadline = peer.deadline;
    let error = peer.finish().unwrap_err();
    assert!(matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ));
    assert!(Instant::now() >= deadline);
    assert!(Instant::now() < deadline + Duration::from_secs(1));
}

#[test]
fn silent_head_reaches_absolute_deadline() {
    assert_deadline(HeadPeer::new());
}

#[test]
fn trickled_head_cannot_extend_absolute_deadline() {
    let mut peer = HeadPeer::new();
    while Instant::now() < peer.deadline {
        if peer.client.write_all(b"x").is_err() {
            break;
        }
        thread::sleep(Duration::from_millis(40));
    }
    assert_deadline(peer);
}

#[test]
fn fragmented_valid_head() {
    let mut peer = HeadPeer::new();
    for part in [
        b"GET /mis".as_slice(),
        b"sing HTTP/1.1\r",
        b"\nHost: loopback\r\n",
        b"\r",
        b"\n",
    ] {
        peer.client.write_all(part).unwrap();
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(peer.finish().unwrap(), "GET /missing HTTP/1.1");
}

#[test]
fn complete_head_survives_write_half_close() {
    let mut peer = HeadPeer::new();
    peer.client.write_all(REQUEST).unwrap();
    peer.client.shutdown(Shutdown::Write).unwrap();
    assert_eq!(peer.finish().unwrap(), "GET /missing HTTP/1.1");
}

#[test]
fn malformed_utf8_is_invalid_data() {
    let mut peer = HeadPeer::new();
    peer.client
        .write_all(b"GET /\xff HTTP/1.1\r\n\r\n")
        .unwrap();
    assert_eq!(
        peer.finish().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn over_cap_head_is_invalid_data() {
    let mut peer = HeadPeer::new();
    peer.client.write_all(&[b'x'; 4097]).unwrap();
    assert_eq!(
        peer.finish().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn disconnected_peer_has_no_complete_head() {
    let peer = HeadPeer::new();
    peer.client.shutdown(Shutdown::Both).unwrap();
    assert_eq!(
        peer.finish().unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn poisoned_listener_returns_io_error() {
    let listener = Mutex::new(TcpListener::bind("127.0.0.1:0").unwrap());
    let _ = std::panic::catch_unwind(|| {
        let _guard = listener.lock().unwrap();
        panic!("poison fixture listener");
    });
    assert_eq!(
        accept_one(&listener).unwrap_err().kind(),
        io::ErrorKind::Other
    );
}
