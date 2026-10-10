// ABOUTME: Exercises DeadlineWriter departures with real host TCP sockets.
// ABOUTME: Checks residual writes, OS errors, descriptor flags and listener ownership.
#![cfg(all(unix, not(feature = "listener-loom")))]

use esp_shazam_core::stream_http::DeadlineWriter;
use esp_shazam_core::stream_wait::Slot;
use socket2::SockRef;
use std::io::{self, Read};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const BOUND: Duration = Duration::from_secs(3);
const PRESSURE_BYTES: usize = 8 << 20;

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (server, _) = listener.accept().unwrap();
    SockRef::from(&server).set_send_buffer_size(4096).unwrap();
    SockRef::from(&client).set_recv_buffer_size(4096).unwrap();
    client.set_read_timeout(Some(BOUND)).unwrap();
    (server, client)
}

fn flags(socket: &TcpStream) -> libc::c_int {
    // SAFETY: socket owns a live descriptor; F_GETFL takes no extra argument.
    let flags = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0, "{}", io::Error::last_os_error());
    flags
}

#[test]
fn peer_write_half_close_preserves_exact_residual_payload() {
    let (server, mut client) = pair();
    client.shutdown(Shutdown::Write).unwrap();
    let mut eof = [0];
    server.set_read_timeout(Some(BOUND)).unwrap();
    assert_eq!((&server).read(&mut eof).unwrap(), 0);
    let payload: Vec<_> = (0..65536).map(|n| (n % 251) as u8).collect();
    let expected = payload.clone();
    let reader = thread::spawn(move || {
        let mut received = vec![0; expected.len()];
        client.read_exact(&mut received).unwrap();
        assert_eq!(received, expected);
    });
    let original = flags(&server);
    let mut writer = DeadlineWriter::new(&server);
    writer
        .write_all_before(&payload, Instant::now() + BOUND)
        .unwrap();
    assert_eq!(writer.diagnostics().accepted_bytes, payload.len() as u64);
    assert_eq!(writer.diagnostics().send_errors, 0);
    assert_eq!(flags(&server), original);
    reader.join().unwrap();
}

#[test]
fn orderly_full_close_returns_a_send_error_without_error_ready_spin() {
    let (server, client) = pair();
    drop(client);
    let mut eof = [0];
    server.set_read_timeout(Some(BOUND)).unwrap();
    assert_eq!((&server).read(&mut eof).unwrap(), 0);
    let original = flags(&server);
    let mut writer = DeadlineWriter::new(&server);
    let started = Instant::now();
    let error = writer
        .write_all_before(&vec![17; PRESSURE_BYTES], started + BOUND)
        .unwrap_err();
    let metrics = writer.diagnostics();
    assert!(started.elapsed() < BOUND);
    assert!(
        matches!(error.raw_os_error(), Some(libc::EPIPE | libc::ECONNRESET)),
        "{error:?}"
    );
    assert_eq!(metrics.send_errors, 1);
    assert!(metrics.accepted_bytes < PRESSURE_BYTES as u64);
    assert!(metrics.poll_error_ready <= 1, "{metrics:?}");
    assert_eq!(flags(&server), original);
    eprintln!("orderly full close: {error:?}; {metrics:?}");
}

#[test]
fn pressured_reset_bounds_error_ready_retry_and_releases_slot_after_socket_drop() {
    for nonblocking in [false, true] {
        let (server, mut client) = pair();
        server.set_nonblocking(nonblocking).unwrap();
        let original = flags(&server);
        let slots = Arc::new(AtomicUsize::new(0));
        let slot = Slot::take(&slots, 1).unwrap();
        let (pressured_tx, pressured_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let payload = vec![73; PRESSURE_BYTES];
            let mut writer = DeadlineWriter::new(&server);
            let error = writer
                .write_all_before(&payload, Instant::now() + Duration::from_millis(50))
                .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            let prefix = writer.diagnostics();
            assert!(prefix.accepted_bytes > 0 && prefix.accepted_bytes < PRESSURE_BYTES as u64);
            assert!(prefix.send_would_block > 0 && prefix.poll_calls > 0);
            pressured_tx.send(prefix.accepted_bytes).unwrap();
            writer.reset_diagnostics();
            let started = Instant::now();
            let error = writer
                .write_all_before(&payload, started + BOUND)
                .unwrap_err();
            let elapsed = started.elapsed();
            let metrics = writer.diagnostics();
            assert_eq!(flags(&server), original);
            assert_eq!(slot.taken(), 1);
            drop(server);
            assert_eq!(slot.taken(), 1);
            drop(slot);
            finished_tx.send((error, elapsed, metrics)).unwrap();
        });
        let accepted = pressured_rx.recv_timeout(BOUND).unwrap();
        assert_eq!(slots.load(Ordering::Acquire), 1);
        assert!(Slot::take(&slots, 1).is_none());
        // Check a real received prefix, without claiming all accepted bytes reached the peer.
        let mut prefix = [0; 1024];
        client.read_exact(&mut prefix).unwrap();
        assert_eq!(prefix, [73; 1024]);
        assert!(accepted >= prefix.len() as u64);
        // Leave a window for the second write to wait; assertions also allow a delayed worker.
        thread::sleep(Duration::from_millis(50));
        SockRef::from(&client)
            .set_linger(Some(Duration::ZERO))
            .unwrap();
        drop(client);
        let (error, elapsed, metrics) = finished_rx
            .recv_timeout(BOUND + Duration::from_secs(1))
            .unwrap();
        worker.join().unwrap();
        assert!(elapsed < BOUND, "{elapsed:?}");
        assert_eq!(error.raw_os_error(), Some(libc::ECONNRESET), "{error:?}");
        assert_eq!(metrics.send_errors, 1);
        // Reset may precede the second send on a delayed worker; that direct error is valid.
        assert!(metrics.poll_error_ready <= 1, "{metrics:?}");
        assert!(
            metrics.poll_error_ready <= metrics.send_would_block,
            "{metrics:?}"
        );
        assert_eq!(metrics.ready_then_would_block, 0, "{metrics:?}");
        assert_eq!(metrics.poll_errors, 0);
        assert_eq!(slots.load(Ordering::Acquire), 0);
        assert!(Slot::take(&slots, 1).is_some());
        eprintln!("pressured reset nonblocking={nonblocking}: {error:?}; {metrics:?}");
    }
}
