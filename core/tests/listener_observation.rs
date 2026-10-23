// ABOUTME: Tests anonymous listener observations using the production registry.
// ABOUTME: Exercises coherent live timing and nonblocking ownership cleanup.
#[allow(dead_code)]
#[path = "../../firmware/src/listener_observation.rs"]
mod listener_observation;
use listener_observation::*;
#[test]
fn live_write_and_completed_receipt_have_distinct_clocks() {
    let registry = Registry::new();
    let owner = registry.register(10).unwrap();
    assert_eq!(registry.sample(|| 11)[0].state, State::Waiting);
    owner.begin(20, 5, SkipCounters::default());
    let writing = registry.sample(|| 30)[0];
    assert_eq!(writing.state, State::Writing);
    assert_eq!(writing.media_age_us, Some(25));
    assert_eq!(writing.write_us, Some(10));
    owner.finish(40, 20, 5, true, SkipCounters::default());
    let ready = registry.sample(|| 100)[0];
    assert_eq!(ready.state, State::Ready);
    assert_eq!(
        (ready.sample_us, ready.media_age_us, ready.write_us),
        (40, Some(35), Some(20))
    );
    let generation = ready.generation;
    drop(owner);
    assert_eq!(registry.sample(|| 110)[0].state, State::Idle);
    let _replacement = registry.register(120).unwrap();
    assert_ne!(registry.sample(|| 120)[0].generation, generation);
}
#[test]
fn stalled_tcp_writer_has_advancing_live_observations() {
    use esp_shazam_core::stream_http::DeadlineWriter;
    use std::{
        net::{TcpListener, TcpStream},
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (server, _) = listener.accept().unwrap();
    server.set_nonblocking(true).unwrap();
    socket2::SockRef::from(&server)
        .set_send_buffer_size(4096)
        .unwrap();
    socket2::SockRef::from(&peer)
        .set_recv_buffer_size(4096)
        .unwrap();
    let registry = Registry::new();
    let launched = Instant::now();
    let now = || launched.elapsed().as_micros() as u64 + 1;
    std::thread::scope(|scope| {
        let owner = registry.register(now()).unwrap();
        let (sent, received) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let now = &now;
        let handle = scope.spawn(move || {
            let started = now();
            owner.begin(started, 0, SkipCounters::default());
            sent.send(()).unwrap();
            let result = DeadlineWriter::new(&server).write_all_before(
                &vec![0; 8 * 1024 * 1024],
                Instant::now() + Duration::from_millis(500),
            );
            owner.finish(now(), started, 0, result.is_ok(), SkipCounters::default());
            sent.send(()).unwrap();
            released.recv().unwrap();
            assert!(result.is_err());
        });
        received.recv().unwrap();
        std::thread::sleep(Duration::from_millis(30));
        let first = registry.sample(now)[0];
        std::thread::sleep(Duration::from_millis(30));
        let second = registry.sample(now)[0];
        assert_eq!(first.state, State::Writing);
        assert_eq!(second.state, State::Writing);
        assert!(second.write_us.unwrap() > first.write_us.unwrap());
        assert!(second.media_age_us.unwrap() > first.media_age_us.unwrap());
        received.recv().unwrap();
        let failed = registry.sample(now)[0];
        assert_eq!(failed.state, State::Unavailable);
        assert_eq!(failed.media_age_us, None);
        assert_eq!(failed.write_us, None);
        release.send(()).unwrap();
        handle.join().unwrap();
    });
    drop(peer);
}
#[test]
fn sample_clock_runs_after_copy_and_preserves_completed_receipt() {
    let registry = Registry::new();
    let owner = registry.register(10).unwrap();
    owner.begin(20, 5, SkipCounters::default());
    let values = registry.sample(|| 30);
    assert_eq!(values[0].write_us, Some(10));
    owner.finish(40, 20, 5, true, SkipCounters::default());
    assert_eq!(registry.sample(|| 100)[0].sample_us, 40);
}
#[test]
fn failed_payload_completion_invalidates_write_while_owner_remains_live() {
    let registry = Registry::new();
    let owner = registry.register(1).unwrap();
    owner.begin(10, 0, SkipCounters::default());
    owner.finish(20, 10, 0, false, SkipCounters::default());
    let failed = registry.sample(|| 30)[0];
    assert_eq!(failed.state, State::Unavailable);
    assert_eq!(failed.media_age_us, None);
    assert_eq!(failed.write_us, None);
    assert!(registry.register(40).is_some());
    drop(owner);
}
#[test]
fn skip_counters_ride_with_every_owner_publication_and_are_null_when_idle() {
    let registry = Registry::new();
    let owner = registry.register(10).unwrap();
    let waiting = registry.sample(|| 11)[0];
    assert_eq!(waiting.state, State::Waiting);
    assert_eq!(
        (waiting.skips, waiting.frames_skipped, waiting.last_skip_us),
        (Some(0), Some(0), Some(0))
    );
    let counters = SkipCounters {
        skips: 2,
        frames_skipped: 333,
        last_skip_us: 19,
    };
    owner.begin(20, 5, counters);
    let writing = registry.sample(|| 30)[0];
    assert_eq!(writing.state, State::Writing);
    assert_eq!(
        (writing.skips, writing.frames_skipped, writing.last_skip_us),
        (Some(2), Some(333), Some(19))
    );
    owner.finish(40, 20, 5, true, counters);
    let ready = registry.sample(|| 100)[0];
    assert_eq!(ready.state, State::Ready);
    assert_eq!(
        (ready.skips, ready.frames_skipped, ready.last_skip_us),
        (Some(2), Some(333), Some(19))
    );
    // A failed write leaves the owner live; its counters are still facts, unlike the timing.
    owner.finish(50, 40, 5, false, counters);
    let failed = registry.sample(|| 110)[0];
    assert_eq!(failed.state, State::Unavailable);
    assert_eq!((failed.media_age_us, failed.write_us), (None, None));
    assert_eq!(
        (failed.skips, failed.frames_skipped, failed.last_skip_us),
        (Some(2), Some(333), Some(19))
    );
    drop(owner);
    let idle = registry.sample(|| 120)[0];
    assert_eq!(idle.state, State::Idle);
    assert_eq!(
        (idle.skips, idle.frames_skipped, idle.last_skip_us),
        (None, None, None)
    );
}
