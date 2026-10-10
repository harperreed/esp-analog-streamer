// ABOUTME: Exercises the production epoch and bounded switch request with real codec/ring state.
// ABOUTME: Covers cancelled commits, admission races and retirement without a device.
#[path = "../../firmware/src/stream_codec.rs"]
mod stream_codec;
#[path = "../../firmware/src/stream_runtime.rs"]
mod stream_runtime;
use esp_shazam_core::stream_buffer::OutputFormat;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use stream_runtime::Runtime;

#[test]
fn switching_retires_old_ring_and_never_reuses_its_bytes() {
    let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let (_, old) = runtime.admit("GET /stream.mp3 HTTP/1.1");
    let old = old.unwrap();
    old.epoch.ring.lock().unwrap().push(&[1, 2, 3]).unwrap();
    let caller = runtime.clone();
    let join = std::thread::spawn(move || {
        caller.switch(
            OutputFormat::Flac,
            true,
            Arc::new(Mutex::new(true)),
            Duration::from_secs(1),
        )
    });
    assert!(worker.wait_and_service(&runtime, Duration::from_secs(1)));
    assert!(join.join().unwrap().unwrap());
    assert!(old.epoch.retired());
    assert_eq!(runtime.active_format(), OutputFormat::Flac);
    assert_eq!(worker.epoch.ring.lock().unwrap().progress(), (0, 0));
    assert_eq!(worker.epoch.ring.lock().unwrap().capacity(), 141);
    assert_eq!(old.epoch.ring.lock().unwrap().capacity(), 500);
}
#[test]
fn timeout_revokes_commit_and_idle_admission_defers() {
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    assert!(runtime
        .switch(
            OutputFormat::Mp3,
            true,
            Arc::new(Mutex::new(true)),
            Duration::from_millis(1)
        )
        .is_err());
    assert!(!worker.wait_and_service(&runtime, Duration::from_secs(1)));
    assert_eq!(runtime.active_format(), OutputFormat::Flac);
    let (_, listener) = runtime.admit("GET /stream.flac HTTP/1.1");
    let caller = runtime.clone();
    let join = std::thread::spawn(move || {
        caller.switch(
            OutputFormat::Mp3,
            false,
            Arc::new(Mutex::new(true)),
            Duration::from_secs(1),
        )
    });
    assert!(!worker.wait_and_service(&runtime, Duration::from_secs(1)));
    assert!(!join.join().unwrap().unwrap());
    assert!(!listener.unwrap().epoch.retired());
}
#[test]
fn cancelled_play_and_recognition_leave_active_unchanged() {
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let caller = runtime.clone();
    let token = Arc::new(Mutex::new(false));
    let join = std::thread::spawn(move || {
        caller.switch(OutputFormat::Mp3, true, token, Duration::from_secs(1))
    });
    assert!(!worker.wait_and_service(&runtime, Duration::from_secs(1)));
    assert!(join.join().unwrap().is_err());
    assert_eq!(runtime.active_format(), OutputFormat::Flac);
    let (runtime, _) = Runtime::new(OutputFormat::Flac, true).unwrap();
    assert!(runtime
        .switch(
            OutputFormat::Mp3,
            true,
            Arc::new(Mutex::new(true)),
            Duration::from_secs(1)
        )
        .unwrap_err()
        .to_string()
        .contains("recognition"));
}

#[test]
fn draining_retired_listener_also_defers_idle_switch() {
    let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let (_, listener) = runtime.admit("GET /stream.mp3 HTTP/1.1");
    let caller = runtime.clone();
    let join = std::thread::spawn(move || {
        caller.switch(
            OutputFormat::Flac,
            true,
            Arc::new(Mutex::new(true)),
            Duration::from_secs(1),
        )
    });
    assert!(worker.wait_and_service(&runtime, Duration::from_secs(1)));
    assert!(join.join().unwrap().unwrap());
    let caller = runtime.clone();
    let join = std::thread::spawn(move || {
        caller.switch(
            OutputFormat::Mp3,
            false,
            Arc::new(Mutex::new(true)),
            Duration::from_secs(1),
        )
    });
    assert!(!worker.wait_and_service(&runtime, Duration::from_secs(1)));
    assert!(!join.join().unwrap().unwrap());
    drop(listener);
}

#[test]
fn repeated_switches_decode_independently_and_release_unheld_epochs() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let (runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    for (index, format) in [
        OutputFormat::Mp3,
        OutputFormat::Flac,
        OutputFormat::Mp3,
        OutputFormat::Flac,
        OutputFormat::Mp3,
    ]
    .into_iter()
    .enumerate()
    {
        if index != 0 {
            let old = Arc::downgrade(&worker.epoch);
            let caller = runtime.clone();
            let join = std::thread::spawn(move || {
                caller.switch(
                    format,
                    true,
                    Arc::new(Mutex::new(true)),
                    Duration::from_secs(1),
                )
            });
            assert!(worker.wait_and_service(&runtime, Duration::from_secs(1)));
            assert!(join.join().unwrap().unwrap());
            assert!(
                old.upgrade().is_none(),
                "no reader holds old epoch; its storage must drop"
            );
        }
        assert_eq!(worker.epoch.format, format);
        let mut stream = stream_codec::stream_header(format);
        let mut expected = Vec::new();
        let block_frames = format.block_frames();
        let mut cursor = worker.epoch.ring.lock().unwrap().cursor_now();
        let mut copied = vec![0; format.max_frame_bytes()];
        for number in 0..16 {
            let pcm: Vec<i32> = (0..2 * block_frames)
                .map(|sample| (((sample + number * 13) % 307) as i32 - 153) << 12)
                .collect();
            expected.extend(pcm.iter().map(|sample| (sample >> 8) as i16));
            for chunk in pcm.chunks(480) {
                worker.encoder.push(chunk);
            }
            worker
                .encoder
                .encode_block(number as u32, &mut worker.bytes)
                .unwrap();
            let mut ring = worker.epoch.ring.lock().unwrap();
            ring.push(&worker.bytes).unwrap();
            let len = ring.read(&mut cursor, &mut copied).unwrap();
            drop(ring);
            let len = stream_codec::prepare_listener_frame(format, &mut copied, len, number as u32)
                .unwrap();
            stream.extend_from_slice(&copied[..len]);
        }
        let demuxer = match format {
            OutputFormat::Flac => "flac",
            OutputFormat::Mp3 => "mp3",
        };
        let mut decoder = Command::new("ffmpeg")
            .args([
                "-nostdin", "-v", "error", "-f", demuxer, "-i", "pipe:0", "-f", "s16le", "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = decoder.stdin.take().unwrap();
        let writer = std::thread::spawn(move || input.write_all(&stream).unwrap());
        let output = decoder.wait_with_output().unwrap();
        writer.join().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), 16 * block_frames * 4);
        if format == OutputFormat::Flac {
            let samples: Vec<i16> = output
                .stdout
                .chunks_exact(2)
                .map(|bytes| i16::from_le_bytes(bytes.try_into().unwrap()))
                .collect();
            assert_eq!(samples, expected);
        }
    }
}

#[test]
fn same_format_play_waits_for_real_slots_after_cancelled_switch() {
    use esp_shazam_core::stream_wait::Slot;
    use std::{
        sync::{atomic::AtomicUsize, mpsc},
        thread,
    };
    let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let slots = Arc::new(AtomicUsize::new(0));
    let first = Slot::take(&slots, 2).unwrap();
    let second = Slot::take(&slots, 2).unwrap();
    let token = Arc::new(Mutex::new(true));
    let caller = runtime.clone();
    let held = slots.clone();
    let permit = token.clone();
    let (started, running) = mpsc::channel();
    let cancelled = thread::spawn(move || {
        started.send(()).unwrap();
        caller.play_format(OutputFormat::Mp3, permit, &held, 2, Duration::from_secs(1))
    });
    running.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(worker.wait_and_service(&runtime, Duration::from_secs(1)));
    assert_eq!(runtime.active_format(), OutputFormat::Mp3);
    *token.lock().unwrap() = false;
    assert!(cancelled.join().unwrap().is_err());
    let caller = runtime.clone();
    let held = slots.clone();
    let (finished, completion) = mpsc::channel();
    let retry = thread::spawn(move || {
        let result = caller.play_format(
            OutputFormat::Mp3,
            Arc::new(Mutex::new(true)),
            &held,
            2,
            Duration::from_secs(1),
        );
        finished.send(result).unwrap();
    });
    assert!(completion.recv_timeout(Duration::from_millis(20)).is_err());
    drop(first);
    assert!(completion
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap());
    retry.join().unwrap();
    assert!(runtime
        .play_format(
            OutputFormat::Mp3,
            Arc::new(Mutex::new(false)),
            &slots,
            2,
            Duration::from_secs(1)
        )
        .is_err());
    let first = Slot::take(&slots, 2).unwrap();
    assert!(runtime
        .play_format(
            OutputFormat::Mp3,
            Arc::new(Mutex::new(true)),
            &slots,
            2,
            Duration::from_millis(10)
        )
        .is_err());
    drop((first, second));
}
