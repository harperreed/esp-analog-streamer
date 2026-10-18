// ABOUTME: Exercises the close reaper with real sockets: the slot is released at hand-off, before the checked close.
// ABOUTME: Covers the two-socket drain bound, the retained failure, event order and the allocation-free hand-off.
#![cfg(not(any(feature = "capture-loom", feature = "listener-loom")))]
#[path = "support/alloc_count.rs"]
mod alloc_count;
#[path = "../../firmware/src/close_reaper.rs"]
mod close_reaper;
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

use close_reaper::{Reaper, DRAINING_LIMIT};
use esp_shazam_core::{
    stream_buffer::OutputFormat,
    stream_wait::{Capacity, Slot},
};
use listener_telemetry::{EndCode, Payload, Queue, Record};
use std::{
    io::{self, Read},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};
use stream_pipeline::{ConnectionResources, DrainingSocket};
use stream_runtime::{Runtime, SwitchPoll, SwitchStart};

/// One accepted connection with its peer end, which reads with a short timeout.
fn accepted(listener: &TcpListener) -> (TcpStream, TcpStream, SocketAddr) {
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let (socket, address) = listener.accept().unwrap();
    (socket, peer, address)
}

fn still_open(peer: &mut TcpStream) -> bool {
    matches!(peer.read(&mut [0]), Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
}

fn shutdown(socket: TcpStream) -> Result<Duration, io::Error> {
    socket.shutdown(Shutdown::Both)?;
    drop(socket);
    Ok(Duration::from_millis(2))
}

fn idle_switch_finishes(runtime: &Runtime, worker: &mut stream_runtime::Worker) -> bool {
    let SwitchStart::Pending(pending) = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("switch must enqueue");
    };
    worker.service(runtime);
    matches!(pending.poll(), SwitchPoll::Finished(Ok(true)))
}

#[test]
fn hand_off_releases_slot_and_lease_before_the_checked_close() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (socket, mut peer, address) = accepted(&listener);
    let slots = Arc::new(Capacity::default());
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let slot = Slot::take(&slots, 2).unwrap();
    let owner = slot.owner_id();
    let resources = ConnectionResources::new(socket, slot, lease);
    let reaper = Reaper::<DrainingSocket<TcpStream>>::new();

    let permit = reaper.reserve().unwrap();
    let draining = resources.release(|| 20);
    assert_eq!(slots.held(), 0, "the slot is free at hand-off");
    assert_eq!(draining.released_us, 20);
    assert_eq!(draining.held, 1, "held counts this socket at its release");
    assert_eq!(draining.owner_id, owner);
    assert!(
        idle_switch_finishes(&runtime, &mut worker),
        "the format lease is free at hand-off"
    );
    assert!(still_open(&mut peer), "the socket drains after release");
    permit.submit(draining);
    assert_eq!(reaper.draining(), 1);

    let queue = Queue::new();
    let entry = reaper.next().unwrap();
    assert!(listener_telemetry::close_and_report(
        entry,
        &queue,
        Some(address),
        || {
            assert_eq!(slots.held(), 0);
            30
        },
        || 1234,
        shutdown
    )
    .is_ok());
    reaper.finished();
    assert_eq!(reaper.draining(), 0);
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    let record = queue.take().unwrap();
    assert_eq!(record.event_us, 30);
    assert_eq!(record.peer, Some(address));
    assert!(matches!(
        record.payload,
        Payload::Closed {
            owner_id,
            sonos_intent: 0,
            elapsed_ms: 2,
            held: 1,
            released_us: 20,
            reaper_stack_free_bytes: 1234,
        } if owner_id == owner
    ));
    assert!(queue.take().is_none());
}

#[test]
fn third_hand_off_waits_with_its_slot_held_until_a_drain_finishes() {
    let slots = Arc::new(Capacity::default());
    let reaper = Arc::new(Reaper::<u64>::new());
    let mut ids = Vec::new();
    for _ in 0..DRAINING_LIMIT {
        let slot = Slot::take(&slots, 2).unwrap();
        let permit = reaper.reserve().unwrap();
        ids.push(slot.owner_id());
        drop(slot);
        permit.submit(ids[ids.len() - 1]);
    }
    assert_eq!(reaper.draining(), DRAINING_LIMIT);
    assert_eq!(slots.held(), 0);
    let third = Slot::take(&slots, 2).unwrap();
    let third_id = third.owner_id();
    let (reserved, reservation) = mpsc::channel();
    let waiter = thread::spawn({
        let reaper = reaper.clone();
        move || {
            let permit = reaper.reserve().unwrap();
            reserved.send(()).unwrap();
            drop(third);
            permit.submit(third_id);
        }
    });
    assert!(
        reservation
            .recv_timeout(Duration::from_millis(100))
            .is_err(),
        "a third hand-off waits while two sockets drain"
    );
    assert_eq!(slots.held(), 1, "the waiting hand-off keeps its slot");
    assert_eq!(reaper.next(), Some(ids[0]));
    reaper.finished();
    reservation.recv_timeout(Duration::from_secs(1)).unwrap();
    waiter.join().unwrap();
    assert_eq!(slots.held(), 0);
    assert_eq!(reaper.draining(), DRAINING_LIMIT);
    assert_eq!(reaper.next(), Some(ids[1]));
    reaper.finished();
    assert_eq!(reaper.next(), Some(third_id));
    reaper.finished();
    assert_eq!(reaper.draining(), 0);
}

#[test]
fn dropped_permit_frees_its_place_and_a_closed_reaper_refuses_hand_off() {
    let reaper = Arc::new(Reaper::<Box<u8>>::new());
    drop(reaper.reserve().unwrap());
    assert_eq!(reaper.draining(), 0);
    reaper.reserve().unwrap().submit(Box::new(1));
    let worker = thread::spawn({
        let reaper = reaper.clone();
        move || {
            let mut closed = Vec::new();
            reaper.work(|socket| closed.push(*socket));
            closed
        }
    });
    reaper.reserve().unwrap().submit(Box::new(2));
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while reaper.draining() > 0 {
        assert!(std::time::Instant::now() < deadline, "reaper did not drain");
        thread::sleep(Duration::from_millis(1));
    }
    reaper.close();
    assert_eq!(worker.join().unwrap(), vec![1, 2]);
    assert!(reaper.reserve().is_none());
    assert!(reaper.next().is_none());
}

#[test]
fn failed_close_retains_the_socket_without_a_false_closed_event() {
    let slots = Arc::new(Capacity::default());
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
    let draining = resources.release(|| 5);
    assert_eq!(slots.held(), 0);
    let queue = Queue::new();
    let failed = listener_telemetry::close_and_report(
        draining,
        &queue,
        None,
        || 9,
        || 0,
        |socket| {
            // An unconnected real TCP socket fails shutdown; retain its owned handle.
            match socket.shutdown(Shutdown::Both) {
                Ok(()) => {
                    drop(socket);
                    Ok(Duration::ZERO)
                }
                Err(error) => Err((socket, error)),
            }
        },
    )
    .expect_err("actual OS shutdown must fail");
    assert!(queue.take().is_none(), "a failed close publishes nothing");
    assert!(idle_switch_finishes(&runtime, &mut worker));
    // The failure keeps the uncertain socket until its owner decides; here, drop.
    drop(failed);
}

#[test]
fn closed_event_orders_departure_release_and_close() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (socket, mut peer, address) = accepted(&listener);
    let slots = Arc::new(Capacity::default());
    let resources = ConnectionResources::new(socket, Slot::take(&slots, 2).unwrap(), None);
    let clock = Arc::new(AtomicU64::new(0));
    let now = {
        let clock = clock.clone();
        move || clock.fetch_add(1, Ordering::SeqCst) + 1
    };
    let queue = Arc::new(Queue::new());
    let reaper = Arc::new(Reaper::<(DrainingSocket<TcpStream>, Option<SocketAddr>)>::new());
    let worker = thread::spawn({
        let (reaper, queue, now) = (reaper.clone(), queue.clone(), now.clone());
        move || {
            reaper.work(|(draining, peer)| {
                listener_telemetry::close_and_report(draining, &queue, peer, &now, || 0, shutdown)
                    .ok()
                    .expect("real checked close");
            })
        }
    });
    queue.publish(Record {
        event_us: now(),
        peer: Some(address),
        payload: Payload::Departure {
            metrics: None,
            tenths: 0,
            end: EndCode::Retired,
            owner_stack_free_bytes: 0,
        },
    });
    let permit = reaper.reserve().unwrap();
    permit.submit((resources.release(&now), Some(address)));
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    reaper.close();
    worker.join().unwrap();
    let departure = queue.take().unwrap();
    let closed = queue.take().unwrap();
    assert!(matches!(departure.payload, Payload::Departure { .. }));
    let Payload::Closed {
        released_us,
        held: 1,
        ..
    } = closed.payload
    else {
        panic!("closed receipt");
    };
    assert!(departure.event_us < released_us);
    assert!(released_us < closed.event_us);
}

#[test]
fn hand_off_makes_no_heap_allocation() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (socket, _peer, address) = accepted(&listener);
    let slots = Arc::new(Capacity::default());
    let (runtime, _worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
    let resources = ConnectionResources::new(socket, Slot::take(&slots, 2).unwrap(), lease);
    let reaper = Reaper::<(DrainingSocket<TcpStream>, Option<SocketAddr>)>::new();
    let ((), allocations) = alloc_count::allocations_during(|| {
        let permit = reaper.reserve().unwrap();
        let draining = resources.release(|| 1);
        permit.submit((draining, Some(address)));
    });
    assert_eq!(allocations, 0, "hand-off allocates nothing");
    assert_eq!(slots.held(), 0);
    assert_eq!(reaper.draining(), 1);
    let (draining, _) = reaper.next().unwrap();
    drop(draining);
    reaper.finished();
}

#[test]
fn reconnecting_peer_is_admitted_while_its_old_socket_lingers() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let slots = Arc::new(Capacity::default());
    let other = Slot::take(&slots, 2).unwrap();
    let (old_socket, mut old_peer, address) = accepted(&listener);
    let resources = ConnectionResources::new(old_socket, Slot::take(&slots, 2).unwrap(), None);
    assert!(Slot::take(&slots, 2).is_none(), "both slots are held");
    let queue = Arc::new(Queue::new());
    let reaper = Arc::new(Reaper::<DrainingSocket<TcpStream>>::new());
    let (closing, entered) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let worker = thread::spawn({
        let (reaper, queue) = (reaper.clone(), queue.clone());
        move || {
            reaper.work(|draining| {
                listener_telemetry::close_and_report(
                    draining,
                    &queue,
                    Some(address),
                    || 2,
                    || 0,
                    |socket| {
                        // A lingering close: the socket stays open until the peer is admitted.
                        closing.send(()).unwrap();
                        released.recv().unwrap();
                        shutdown(socket)
                    },
                )
                .ok()
                .expect("real checked close");
            })
        }
    });
    let permit = reaper.reserve().unwrap();
    permit.submit(resources.release(|| 1));
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    // The same peer reconnects at once and is admitted while its old socket drains.
    let (new_socket, mut new_peer, _) = accepted(&listener);
    let slot = Slot::take(&slots, 2).expect("admitted while the old socket drains");
    assert_eq!(slots.held(), 2);
    assert!(
        still_open(&mut old_peer),
        "the old socket is still draining"
    );
    assert!(queue.take().is_none(), "no close receipt yet");
    release.send(()).unwrap();
    old_peer
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    assert_eq!(old_peer.read(&mut [0]).unwrap(), 0);
    reaper.close();
    worker.join().unwrap();
    assert!(matches!(
        queue.take().unwrap().payload,
        Payload::Closed {
            held: 2,
            released_us: 1,
            ..
        }
    ));
    let replacement = ConnectionResources::new(new_socket, slot, None);
    drop(replacement.release(|| 3));
    assert_eq!(new_peer.read(&mut [0]).unwrap(), 0);
    drop(other);
    assert_eq!(slots.held(), 0);
}
