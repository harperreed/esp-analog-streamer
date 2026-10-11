// ABOUTME: Checks bounded listener reporting against real socket and admission ownership.
// ABOUTME: A stalled exporter must never retain listener capacity or delay peer EOF.
#![cfg(not(any(feature = "capture-loom", feature = "listener-loom")))]
#[allow(dead_code)]
#[path = "../../firmware/src/listener_telemetry.rs"]
mod listener_telemetry;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_codec.rs"]
mod stream_codec;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_pipeline.rs"]
mod stream_pipeline;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_runtime.rs"]
mod stream_runtime;

use esp_shazam_core::{stream_buffer::OutputFormat, stream_wait::Slot};
use listener_telemetry::{Payload, Queue, Record};
use std::{
    io::Read,
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    },
    time::Duration,
};
use stream_pipeline::ConnectionResources;
use stream_runtime::{Runtime, SwitchPoll, SwitchStart};

fn record(n: u64) -> Record {
    Record {
        event_us: n,
        peer: Some("127.0.0.1:1234".parse().unwrap()),
        payload: Payload::Closed {
            elapsed_ms: n,
            held: 1,
        },
    }
}

#[test]
fn startup_initialization_preserves_records_and_loss_counters() {
    let queue = Queue::new();
    queue.initialize();
    assert!(queue.take().is_none());
    for n in 0..8 {
        assert!(queue.publish(record(n)));
    }
    assert!(!queue.publish(record(8)));
    queue.initialize();
    assert_eq!(queue.dropped(), 1);
    assert_eq!(queue.dropped_full(), 1);
    assert!(queue.loss_occurred());
    for n in 0..8 {
        assert_eq!(queue.take().unwrap().event_us, n);
    }
    assert!(queue.take().is_none());
}

#[test]
fn bounded_fifo_loses_excess_without_overwriting_retained_events() {
    let q = Queue::new();
    for n in 0..8 {
        assert!(q.publish(record(n)));
    }
    assert!(!q.publish(record(99)));
    assert_eq!(q.dropped(), 1);
    for n in 0..8 {
        assert_eq!(q.take().unwrap().event_us, n);
    }
    assert!(q.take().is_none());
    for n in 8..24 {
        assert!(q.publish(record(n)));
        assert_eq!(q.take().unwrap().event_us, n);
    }
}

#[test]
fn blocked_exporter_cannot_delay_real_close_slot_or_format_lease_release() {
    let q = Arc::new(Queue::new());
    assert!(q.publish(record(1)));
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let consumer = {
        let q = q.clone();
        let entered = entered.clone();
        let release = release.clone();
        std::thread::spawn(move || {
            assert_eq!(q.take().unwrap().event_us, 1);
            entered.wait();
            release.wait();
        })
    };
    entered.wait();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let slots = Arc::new(AtomicUsize::new(0));
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let resources = ConnectionResources::new(socket, Slot::take(&slots, 2).unwrap(), lease);
    // A full queue is deliberately not drained while actual cleanup runs.
    for n in 2..10 {
        assert!(q.publish(record(n)));
    }
    let close = listener_telemetry::close_and_report(
        resources,
        &q,
        record(10).peer,
        || 10,
        |socket| {
            let start = std::time::Instant::now();
            socket.shutdown(std::net::Shutdown::Both)?;
            drop(socket);
            Ok::<_, std::io::Error>(start.elapsed())
        },
    );
    assert!(close.is_ok());
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    assert_eq!(slots.load(Ordering::Acquire), 0);
    assert_eq!(q.dropped(), 1);
    let SwitchStart::Pending(pending) = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("switch must enqueue");
    };
    worker.service(&runtime);
    assert!(matches!(pending.poll(), SwitchPoll::Finished(Ok(true))));
    release.wait();
    consumer.join().unwrap();
}

#[test]
fn admitted_refusal_sends_original_response_and_records_numeric_write_failure() {
    use esp_shazam_core::stream_http::Status;
    use listener_telemetry::RefusalReason;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let (socket, address) = listener.accept().unwrap();
    let q = Queue::new();
    let response = Status::ServiceUnavailable.response();
    listener_telemetry::send_admitted_refusal(
        &socket,
        &q,
        Some(address),
        RefusalReason::BufferedStartup,
        Duration::from_millis(50),
        &response,
        || 44,
    );
    drop(socket);
    let mut got = String::new();
    peer.read_to_string(&mut got).unwrap();
    assert_eq!(got, response);
    assert!(matches!(
        q.take().unwrap().payload,
        Payload::Refusal {
            reason: RefusalReason::BufferedStartup,
            response_error: None,
            ..
        }
    ));
    assert!(q.take().is_none());
}

#[test]
fn timeout_result_is_reported_without_consuming_or_changing_terminal_error() {
    let q = Queue::new();
    let result = Err(std::io::Error::from(std::io::ErrorKind::TimedOut));
    listener_telemetry::report_timeout(&q, record(1).peer, &result, 2500, 2400, 99, || 45);
    assert!(matches!(
        q.take().unwrap().payload,
        Payload::Timeout {
            elapsed_ms: 2500,
            limit_ms: 2400,
            bytes: 99
        }
    ));
    assert!(matches!(&result, Err(error) if error.kind() == std::io::ErrorKind::TimedOut));
    listener_telemetry::report_timeout(&q, record(1).peer, &Ok(()), 1, 2400, 2, || 46);
    assert!(q.take().is_none());
}

#[test]
fn real_failed_close_retains_slot_and_format_lease_without_false_success_event() {
    let q = Queue::new();
    let slots = Arc::new(AtomicUsize::new(0));
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .unwrap();
    let resources = ConnectionResources::new(
        TcpStream::from(socket),
        Slot::take(&slots, 2).unwrap(),
        lease,
    );
    let failed = listener_telemetry::close_and_report(
        resources,
        &q,
        None,
        || 20,
        |socket| {
            // An unconnected real TCP socket fails shutdown; retain its owned handle.
            match socket.shutdown(std::net::Shutdown::Both) {
                Ok(()) => {
                    drop(socket);
                    Ok(Duration::ZERO)
                }
                Err(error) => Err((socket, error)),
            }
        },
    )
    .expect_err("actual OS shutdown must fail");
    assert_eq!(slots.load(Ordering::Acquire), 1);
    assert!(q.take().is_none());
    let SwitchStart::Pending(pending) = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("switch must enqueue");
    };
    worker.service(&runtime);
    assert!(matches!(pending.poll(), SwitchPoll::Finished(Ok(false))));
    assert_eq!(runtime.active_format(), OutputFormat::Flac);
    drop(failed);
    assert_eq!(slots.load(Ordering::Acquire), 0);
}

#[test]
fn real_socket_timeout_and_refusal_reach_close_with_full_queue_and_blocked_exporter() {
    use esp_shazam_core::stream_http::{DeadlineWriter, Status};
    use listener_telemetry::RefusalReason;
    let q = Arc::new(Queue::new());
    q.publish(record(1));
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let exporter = {
        let q = q.clone();
        std::thread::spawn(move || {
            q.take().unwrap();
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        })
    };
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    for n in 0..8 {
        assert!(q.publish(record(n)));
    }
    for refusal in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let (socket, address) = listener.accept().unwrap();
        let slots = Arc::new(AtomicUsize::new(0));
        let (runtime, _worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
        let resources = ConnectionResources::new(socket, Slot::take(&slots, 2).unwrap(), lease);
        if refusal {
            let epoch = &resources.lease.as_ref().unwrap().epoch;
            let mut actor = stream_pipeline::CompressedListener::new(
                epoch,
                Some(esp_shazam_core::stream_buffer::BufferPreset::Medium),
                false,
                &mut || Duration::ZERO,
            );
            assert!(matches!(
                actor.step(epoch, &mut [], &mut || Duration::from_secs(8), &mut || true),
                stream_pipeline::ListenerStep::End(stream_pipeline::End::CaptureStalled)
            ));
            assert!(actor.refuse_buffered);
            listener_telemetry::send_admitted_refusal(
                &resources.socket,
                &q,
                Some(address),
                RefusalReason::BufferedStartup,
                Duration::from_millis(50),
                &Status::ServiceUnavailable.response(),
                || 30,
            );
        } else {
            socket2::SockRef::from(&resources.socket)
                .set_send_buffer_size(4096)
                .unwrap();
            socket2::SockRef::from(&peer)
                .set_recv_buffer_size(4096)
                .unwrap();
            let mut writer = DeadlineWriter::new(&resources.socket);
            let result = writer.write_all_before(
                &vec![5; 4 * 1024 * 1024],
                std::time::Instant::now() + Duration::from_millis(30),
            );
            assert!(
                matches!(&result,Err(error) if matches!(error.kind(),std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock))
            );
            let epoch = &resources.lease.as_ref().unwrap().epoch;
            let mut actor =
                stream_pipeline::CompressedListener::new(epoch, None, false, &mut || {
                    Duration::ZERO
                });
            assert!(matches!(
                actor.step(epoch, &mut [], &mut || Duration::ZERO, &mut || true),
                stream_pipeline::ListenerStep::Progress
            ));
            assert!(matches!(
                actor.step(epoch, &mut [], &mut || Duration::ZERO, &mut || true),
                stream_pipeline::ListenerStep::Write { header: true, .. }
            ));
            let result = writer.write_all_before(
                actor.write_bytes(&[]),
                std::time::Instant::now() + Duration::from_millis(30),
            );
            assert!(
                matches!(&result,Err(error) if matches!(error.kind(),std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock))
            );
            listener_telemetry::report_timeout(
                &q,
                Some(address),
                &result,
                30,
                30,
                actor.write_bytes(&[]).len(),
                || 30,
            );
            assert!(matches!(
                actor.write_finished(result, &mut || Duration::ZERO),
                stream_pipeline::ListenerStep::End(stream_pipeline::End::StoppedReading)
            ));
        }
        assert!(listener_telemetry::close_and_report(
            resources,
            &q,
            Some(address),
            || 40,
            |socket| {
                let start = std::time::Instant::now();
                socket.shutdown(std::net::Shutdown::Both)?;
                drop(socket);
                Ok::<_, std::io::Error>(start.elapsed())
            }
        )
        .is_ok());
        assert_eq!(slots.load(Ordering::Acquire), 0);
        let mut bytes = Vec::new();
        peer.read_to_end(&mut bytes).unwrap();
        if refusal {
            assert_eq!(bytes, Status::ServiceUnavailable.response().as_bytes());
        }
    }
    assert_eq!(q.dropped(), 4);
    release_tx.send(()).unwrap();
    exporter.join().unwrap();
}

#[test]
fn fixed_departure_keeps_sender_counters_and_error_identity_without_heap_messages() {
    use esp_shazam_core::stream_http::WriterDiagnostics;
    use listener_telemetry::{Departure, EndCode, ErrorCode};
    let q = Queue::new();
    let error = std::io::Error::from_raw_os_error(32);
    let record = Record {
        event_us: 123,
        peer: None,
        payload: Payload::Departure {
            metrics: Some(Departure {
                media: Default::default(),
                elapsed_us: 20,
                task_runtime_us: 7,
                runtime_valid: true,
                frames_sent: 2,
                bytes_sent: 1234,
                frame_write_wall_total_us: 9,
                frame_write_wall_max_us: 8,
                frame_read_wait_wall_total_us: 3,
                payload_elapsed_us: 18,
                block_samples: 1152,
                socket: WriterDiagnostics {
                    accepted_bytes: 1234,
                    send_calls: 5,
                    poll_calls: 2,
                    ..Default::default()
                },
            }),
            tenths: 1,
            end: EndCode::Failed(ErrorCode::from(&error)),
            owner_stack_free_bytes: 512,
        },
    };
    assert!(q.publish(record));
    let got = q.take().unwrap();
    assert_eq!(got.event_us, 123);
    let Payload::Departure {
        metrics: Some(metrics),
        end: EndCode::Failed(error),
        ..
    } = got.payload
    else {
        panic!("departure fields lost");
    };
    assert_eq!(
        (
            metrics.frames_sent,
            metrics.bytes_sent,
            metrics.block_samples
        ),
        (2, 1234, 1152)
    );
    assert_eq!(
        (
            metrics.socket.send_calls,
            metrics.socket.poll_calls,
            metrics.socket.accepted_bytes
        ),
        (5, 2, 1234)
    );
    assert_eq!(error.raw_os_error, Some(32));
    assert_eq!(error.kind, std::io::ErrorKind::BrokenPipe);
    println!(
        "record_bytes={} queue_bytes={}",
        std::mem::size_of::<Record>(),
        std::mem::size_of::<Queue>()
    );
}

#[test]
fn delayed_actual_close_holds_admission_until_socket_operation_finishes() {
    let q = Arc::new(Queue::new());
    for n in 0..8 {
        assert!(q.publish(record(n)));
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    let (socket, address) = listener.accept().unwrap();
    let slots = Arc::new(AtomicUsize::new(0));
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let resources = ConnectionResources::new(socket, Slot::take(&slots, 2).unwrap(), lease);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel();
    let closer = {
        let q = q.clone();
        std::thread::spawn(move || {
            listener_telemetry::close_and_report(
                resources,
                &q,
                Some(address),
                || 50,
                |socket| {
                    let started = std::time::Instant::now();
                    started_tx.send(()).unwrap();
                    finish_rx.recv().unwrap();
                    socket.shutdown(std::net::Shutdown::Both)?;
                    drop(socket);
                    Ok::<_, std::io::Error>(started.elapsed())
                },
            )
            .is_ok()
        })
    };
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(slots.load(Ordering::Acquire), 1);
    let SwitchStart::Pending(pending) = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("switch must enqueue");
    };
    worker.service(&runtime);
    assert!(matches!(pending.poll(), SwitchPoll::Finished(Ok(false))));
    assert!(
        matches!(peer.read(&mut [0]),Err(error) if matches!(error.kind(),std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
    );
    finish_tx.send(()).unwrap();
    assert!(closer.join().unwrap());
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    assert_eq!(slots.load(Ordering::Acquire), 0);
    assert_eq!(q.dropped(), 1);
}

#[test]
fn admitted_allocation_refusal_preserves_real_response_write_error() {
    use esp_shazam_core::stream_http::Status;
    use listener_telemetry::{ErrorCode, RefusalReason};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, address) = listener.accept().unwrap();
    socket.shutdown(std::net::Shutdown::Write).unwrap();
    let q = Queue::new();
    let allocation = Vec::<u8>::new().try_reserve(usize::MAX).unwrap_err();
    let allocation = std::io::Error::other(allocation);
    listener_telemetry::send_admitted_refusal(
        &socket,
        &q,
        Some(address),
        RefusalReason::BufferAllocation(ErrorCode::from(&allocation)),
        Duration::from_millis(50),
        &Status::ServiceUnavailable.response(),
        || 88,
    );
    let Payload::Refusal {
        reason: RefusalReason::BufferAllocation(error),
        response_error: Some(write),
        status,
    } = q.take().unwrap().payload
    else {
        panic!("numeric errors must survive refusal");
    };
    assert_eq!(status, 503);
    assert_eq!(error.kind, std::io::ErrorKind::Other);
    assert_eq!(write.kind, std::io::ErrorKind::BrokenPipe);
    assert!(write.raw_os_error.is_some());
    drop(socket);
    drop(peer);
}

fn socket_flags(socket: &TcpStream) -> i32 {
    use std::os::fd::AsRawFd;
    // SAFETY: F_GETFL only reads flags from this live, borrowed descriptor.
    unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFL) }
}

// Real peer progress keeps the legacy blocking write alive past a whole-response budget.
fn with_bursty_refusal_peer(send: impl FnOnce(&TcpStream), response_len: usize) -> Duration {
    use std::{net::Shutdown, sync::mpsc, thread, time::Instant};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).unwrap();
    client.set_recv_buffer_size(64 << 10).unwrap();
    client
        .connect(&listener.local_addr().unwrap().into())
        .unwrap();
    let (server, _) = listener.accept().unwrap();
    socket2::SockRef::from(&server)
        .set_send_buffer_size(64 << 10)
        .unwrap();
    let mut client: TcpStream = client.into();
    client
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let flags = socket_flags(&server);
    let (ready_tx, ready_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let horizon = Instant::now() + Duration::from_secs(4);
        let mut total = 0;
        let mut buffer = [0; 64 << 10];
        ready_tx.send(()).unwrap();
        while Instant::now() < horizon {
            if stop_rx.recv_timeout(Duration::from_millis(300)).is_ok() {
                break;
            }
            let mut burst = 0;
            while burst < buffer.len() && Instant::now() < horizon {
                match client.read(&mut buffer[burst..]) {
                    Ok(0) => return total + burst,
                    Ok(n) => burst += n,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                        ) =>
                    {
                        break
                    }
                    Err(_) => return total + burst,
                }
            }
            total += burst;
        }
        // Leave the peer open without progress until the legacy send timeout expires.
        let _ = stop_rx.recv_timeout(Duration::from_secs(1));
        total
    });
    ready_rx.recv().unwrap();
    let started = Instant::now();
    send(&server);
    let took = started.elapsed();
    let restored_flags = socket_flags(&server);
    let shutdown = server.shutdown(Shutdown::Both);
    let _ = stop_tx.send(());
    let received = reader.join().unwrap();
    shutdown.unwrap();
    assert!(flags >= 0);
    assert_eq!(restored_flags, flags);
    assert!(
        received > 0 && received < response_len,
        "peer received {received} bytes"
    );
    took
}

#[test]
fn admitted_refusal_uses_one_budget_while_peer_keeps_reading() {
    use listener_telemetry::RefusalReason;
    let response = "x".repeat(8 << 20);
    let queue = Queue::new();
    let took = with_bursty_refusal_peer(
        |socket| {
            listener_telemetry::send_admitted_refusal(
                socket,
                &queue,
                record(1).peer,
                RefusalReason::BufferedStartup,
                Duration::from_millis(400),
                &response,
                || 55,
            );
        },
        response.len(),
    );
    let event = queue.take().unwrap();
    assert_eq!(event.event_us, 55);
    assert_eq!(event.peer, record(1).peer);
    let Payload::Refusal {
        reason,
        status,
        response_error: Some(error),
    } = event.payload
    else {
        panic!("pressure must record the actual response-write failure");
    };
    assert!(matches!(reason, RefusalReason::BufferedStartup));
    assert_eq!(status, 503);
    assert!(
        matches!(
            error.kind,
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ),
        "{error:?}"
    );
    assert!(queue.take().is_none());
    assert!(
        took >= Duration::from_millis(350) && took < Duration::from_millis(700),
        "whole refusal write took {took:?}"
    );
    assert_eq!(error.kind, std::io::ErrorKind::TimedOut);
    assert_eq!(error.raw_os_error, None);
}

#[test]
fn shared_response_write_preserves_exact_bytes_and_socket_flags() {
    use esp_shazam_core::stream_http::write_response;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let flags = socket_flags(&socket);
    let response =
        b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
    let result = write_response(&socket, response, Duration::from_millis(400));
    let restored_flags = socket_flags(&socket);
    socket.shutdown(std::net::Shutdown::Write).unwrap();
    let mut received = Vec::new();
    peer.read_to_end(&mut received).unwrap();
    result.unwrap();
    assert!(flags >= 0);
    assert_eq!(restored_flags, flags);
    assert_eq!(received, response);
}

#[test]
fn shared_response_write_times_out_with_original_socket_flags() {
    use esp_shazam_core::stream_http::write_response;
    let response = vec![b'x'; 8 << 20];
    let mut result = Ok(());
    let took = with_bursty_refusal_peer(
        |socket| {
            result = write_response(socket, &response, Duration::from_millis(400));
        },
        response.len(),
    );
    let error = result.expect_err("large response must exceed the write budget");
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(error.raw_os_error(), None);
    assert!(
        took >= Duration::from_millis(350) && took < Duration::from_millis(700),
        "whole response write took {took:?}"
    );
}

#[test]
fn encoded_terminal_identity_survives_deferred_queue_export() {
    use listener_telemetry::EndCode;
    use stream_pipeline::End;
    for end in [
        End::EncodedReset,
        End::InvalidCursor,
        End::Retired,
        End::FellBehind,
    ] {
        let expected = format!("{end:?}");
        let queue = Queue::new();
        let mut event = record(42);
        event.payload = Payload::Departure {
            metrics: None,
            tenths: 3,
            end: EndCode::from(&end),
            owner_stack_free_bytes: 512,
        };
        assert!(queue.publish(event));
        let exported = queue.take().unwrap();
        let Payload::Departure {
            end,
            tenths,
            owner_stack_free_bytes,
            ..
        } = exported.payload
        else {
            panic!("departure")
        };
        assert_eq!(format!("{end:?}"), expected);
        assert_eq!((tenths, owner_stack_free_bytes), (3, 512));
        assert_eq!(exported.event_us, event.event_us);
        assert!(queue.take().is_none());
    }
}

#[test]
fn actual_listener_launch_failure_closes_captured_resources() {
    // This host owner exercises the existing close seam on closure destruction;
    // ESP checked-close and ListenerConnection itself remain target acceptance work.
    struct ClosingOwner {
        resources: Option<ConnectionResources<TcpStream>>,
        queue: Arc<Queue>,
    }
    impl Drop for ClosingOwner {
        fn drop(&mut self) {
            let closed = listener_telemetry::close_and_report(
                self.resources.take().unwrap(),
                &self.queue,
                None,
                || 30,
                |socket| {
                    socket.shutdown(std::net::Shutdown::Both)?;
                    drop(socket);
                    Ok::<_, std::io::Error>(Duration::ZERO)
                },
            );
            assert!(closed.is_ok());
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let slots = Arc::new(AtomicUsize::new(0));
    let queue = Arc::new(Queue::new());
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let owner = ClosingOwner {
        resources: Some(ConnectionResources::new(
            socket,
            Slot::take(&slots, 2).unwrap(),
            lease,
        )),
        queue: queue.clone(),
    };
    let launched = std::thread::Builder::new()
        .stack_size(usize::MAX)
        .spawn(move || drop(owner));
    assert!(launched.is_err());
    assert_eq!(slots.load(Ordering::Acquire), 0);
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    assert!(
        queue.take().is_some(),
        "close publishes after releasing resources"
    );
    assert!(
        queue.take().is_none(),
        "the captured owner closes exactly once"
    );
    let SwitchStart::Pending(pending) = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("switch must enqueue");
    };
    worker.service(&runtime);
    assert!(matches!(pending.poll(), SwitchPoll::Finished(Ok(true))));
}
