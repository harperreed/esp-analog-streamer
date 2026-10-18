// ABOUTME: Exercises the production fixed listener mailbox with real worker threads.
// ABOUTME: Verifies startup gating, rapid dispatch, reuse and owned rejection.
#[path = "../../firmware/src/stream_workers.rs"]
mod stream_workers;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};
use stream_workers::Mailbox;

#[test]
fn permanent_workers_dispatch_two_rapid_jobs_and_reuse() {
    let mailbox = Arc::new(Mailbox::new());
    let (sent, received) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let blocked = Arc::new(std::sync::Mutex::new(blocked));
    let workers: Vec<_> = (0..2)
        .map(|index| {
            let mailbox = mailbox.clone();
            let sent = sent.clone();
            let blocked = blocked.clone();
            thread::spawn(move || {
                mailbox.work(index, |job| {
                    sent.send((job, thread::current().id())).unwrap();
                    if job < 2 {
                        blocked.lock().unwrap().recv().unwrap();
                    }
                })
            })
        })
        .collect();
    assert!(mailbox.wait_ready(Instant::now() + Duration::from_secs(2)));
    mailbox.try_submit(0).unwrap();
    mailbox.try_submit(1).unwrap();
    mailbox.run();
    let first = received.recv_timeout(Duration::from_secs(2)).unwrap();
    let second = received.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_ne!(first.1, second.1);
    release.send(()).unwrap();
    release.send(()).unwrap();
    for job in 2..20 {
        mailbox.try_submit(job).unwrap();
        let actual = received.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(actual.0, job);
        assert!([first.1, second.1].contains(&actual.1));
    }
    mailbox.close();
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
fn partial_startup_exits_without_dispatch_and_returns_owned_job() {
    let mailbox = Arc::new(Mailbox::new());
    let worker_mailbox = mailbox.clone();
    let worker =
        thread::spawn(move || worker_mailbox.work(0, |_: Box<u8>| panic!("preparing dispatched")));
    assert!(!mailbox.wait_ready(Instant::now() + Duration::from_millis(20)));
    assert!(thread::Builder::new()
        .stack_size(usize::MAX)
        .spawn(|| {})
        .is_err());
    mailbox.close();
    worker.join().unwrap();
    let job = Box::new(9);
    let address = &*job as *const u8;
    let job = mailbox.try_submit(job).unwrap_err();
    assert_eq!(address, &*job as *const u8);
    assert_eq!(Arc::strong_count(&mailbox), 1);
}

#[allow(dead_code)]
#[path = "../../firmware/src/stream_codec.rs"]
mod stream_codec;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_pipeline.rs"]
mod stream_pipeline;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_runtime.rs"]
mod stream_runtime;

struct SocketJob {
    resources: Option<stream_pipeline::ConnectionResources<std::net::TcpStream>>,
    mailbox: std::sync::Weak<Mailbox<SocketJob>>,
}
impl Drop for SocketJob {
    fn drop(&mut self) {
        // Reentering the mailbox proves checked close does not hold its mutex.
        if let Some(mailbox) = self.mailbox.upgrade() {
            let _ = mailbox.wait_ready(Instant::now());
        }
        self.resources
            .take()
            .unwrap()
            .release(|| 0)
            .close(|socket| {
                socket.shutdown(std::net::Shutdown::Both)?;
                drop(socket);
                Ok::<_, std::io::Error>(())
            })
            .ok()
            .expect("real checked close");
    }
}

#[test]
fn queued_leases_and_rejected_owners_close_outside_mailbox_lock() {
    use esp_shazam_core::{
        stream_buffer::OutputFormat,
        stream_wait::{Capacity, Slot},
    };
    use std::{
        io::Read,
        net::{TcpListener, TcpStream},
    };
    use stream_runtime::{Runtime, SwitchPoll, SwitchStart};
    let mailbox = Arc::new(Mailbox::new());
    let slots = Arc::new(Capacity::default());
    let (runtime, mut encoder) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peers = Vec::new();
    for _ in 0..2 {
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        peers.push(peer);
        let (socket, _) = listener.accept().unwrap();
        let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
        let job = SocketJob {
            resources: Some(stream_pipeline::ConnectionResources::new(
                socket,
                Slot::take(&slots, 2).unwrap(),
                lease,
            )),
            mailbox: Arc::downgrade(&mailbox),
        };
        assert!(mailbox.try_submit(job).is_ok());
    }
    assert_eq!(slots.held(), 2);
    let SwitchStart::Pending(pending) = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("queued switch");
    };
    encoder.service(&runtime);
    assert!(matches!(pending.poll(), SwitchPoll::Finished(Ok(false))));
    mailbox.close();
    assert_eq!(slots.held(), 0);
    for peer in &mut peers {
        assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    }
    let SwitchStart::Pending(pending) = runtime.begin_switch(
        OutputFormat::Mp3,
        false,
        Arc::new(std::sync::Mutex::new(true)),
        Duration::from_secs(1),
    ) else {
        panic!("released switch");
    };
    encoder.service(&runtime);
    assert!(matches!(pending.poll(), SwitchPoll::Finished(Ok(true))));
    // An idle mailbox holds no runtime lease; rejection retains ownership until caller drop.
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let job = SocketJob {
        resources: Some(stream_pipeline::ConnectionResources::new(
            socket,
            Slot::take(&slots, 2).unwrap(),
            None,
        )),
        mailbox: Arc::downgrade(&mailbox),
    };
    let rejected = mailbox
        .try_submit(job)
        .expect_err("closed mailbox returns owner");
    assert_eq!(slots.held(), 1);
    drop(rejected);
    assert_eq!(slots.held(), 0);
    drop(peer);
}

#[allow(dead_code)]
#[path = "../../firmware/src/stream_admission.rs"]
mod stream_admission;

#[test]
fn failure_after_admission_spawn_wakes_all_created_gated_workers() {
    let mailbox = Arc::new(Mailbox::<usize>::new());
    let gate = Arc::new(stream_admission::StartupGate::new());
    let mut workers = Vec::new();
    for index in 0..2 {
        let mailbox = mailbox.clone();
        workers.push(thread::spawn(move || {
            mailbox.work(index, |_| panic!("startup dispatched"))
        }));
    }
    assert!(mailbox.wait_ready(Instant::now() + Duration::from_secs(2)));
    let admission_gate = gate.clone();
    workers.push(thread::spawn(move || assert!(!admission_gate.wait())));
    // An actual host spawn failure exercises the rollback decision, not target
    // allocator behavior. Already-created workers must never reach Run.
    assert!(thread::Builder::new()
        .stack_size(usize::MAX)
        .spawn(|| {})
        .is_err());
    gate.exit();
    mailbox.close();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(Arc::strong_count(&mailbox), 1);
    assert_eq!(Arc::strong_count(&gate), 1);
}

#[test]
fn full_mailbox_returns_exact_owner_and_accepts_between_worker_turns() {
    let mailbox = Arc::new(Mailbox::new());
    mailbox.try_submit(Box::new(0)).unwrap();
    mailbox.try_submit(Box::new(1)).unwrap();
    let third = Box::new(2_usize);
    let address = &*third as *const usize;
    let third = mailbox.try_submit(third).unwrap_err();
    assert_eq!(address, &*third as *const usize);
    let (sent, received) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let worker_mailbox = mailbox.clone();
    let worker = thread::spawn(move || {
        worker_mailbox.work(0, |job| {
            let value = *job;
            drop(job);
            sent.send(value).unwrap();
            released.recv_timeout(Duration::from_secs(2)).unwrap();
        })
    });
    mailbox.run();
    assert_eq!(received.recv_timeout(Duration::from_secs(2)).unwrap(), 0);
    // Worker has dropped its previous job but is paused before its next receive.
    mailbox.try_submit(third).unwrap();
    release.send(()).unwrap();
    assert_eq!(received.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
    release.send(()).unwrap();
    assert_eq!(received.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
    mailbox.close();
    release.send(()).unwrap();
    worker.join().unwrap();
}
