// ABOUTME: Drives the real compressed listener through live-policy skips under virtual time.
// ABOUTME: Checks frame-boundary resync, stale drops, end precedence and a real slow socket.
#![cfg(not(any(feature = "capture-loom", feature = "listener-loom")))]
#[path = "support/alloc_count.rs"]
mod alloc_count;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_codec.rs"]
mod stream_codec;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_pipeline.rs"]
mod stream_pipeline;
#[allow(dead_code)]
#[path = "../../firmware/src/stream_runtime.rs"]
mod stream_runtime;

use esp_shazam_core::{
    flac_ring::{FlacRing, SourceMetadata},
    network_lifecycle::Network,
    stream_buffer::OutputFormat,
    stream_http::{DeadlineWriter, ListenerPolicy},
};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use stream_pipeline::{CompressedListener, End, ListenerStep};

use stream_runtime::{Epoch, Runtime, Worker};

fn source_at(us: u64) -> SourceMetadata {
    SourceMetadata {
        raw_generation: 0,
        source_start: 0,
        source_end: 1,
        oldest_accepted_us: us,
        raw_publication_us: us,
    }
}

/// Encodes one block of deterministic PCM and publishes it; returns the 16-bit samples ffmpeg
/// should decode for it.
fn publish_block(worker: &mut Worker, epoch: &Epoch, number: usize, at: u64) -> Vec<i16> {
    let block_frames = epoch.format.block_frames();
    let pcm = block_pcm(block_frames, number);
    for chunk in pcm.chunks(480) {
        worker.encoder.push(chunk);
    }
    worker
        .encoder
        .encode_block(number as u32, &mut worker.bytes)
        .unwrap();
    epoch
        .ring
        .lock()
        .unwrap()
        .push(&worker.bytes, source_at(at), || at)
        .unwrap();
    pcm.iter().map(|sample| (sample >> 8) as i16).collect()
}

/// Deterministic 24-bit PCM for one block, distinct per block number.
fn block_pcm(block_frames: usize, number: usize) -> Vec<i32> {
    (0..2 * block_frames)
        .map(|sample| (((sample + number * 13) % 307) as i32 - 153) << 12)
        .collect()
}

/// Parses a FLAC frame header's UTF-8 coded block number.
fn flac_number(frame: &[u8]) -> u32 {
    assert!(frame.len() >= 7 && frame[0] == 0xff);
    if frame[4] < 128 {
        return u32::from(frame[4]);
    }
    let count = frame[4].leading_ones() as usize;
    let mut value = u32::from(frame[4] & (0x7f >> count));
    for &byte in &frame[5..4 + count] {
        value = (value << 6) | u32::from(byte & 63);
    }
    value
}

/// Takes a listener through admission and the header write, appending the head to `stream`.
fn admit_live(
    epoch: &Epoch,
    policy: ListenerPolicy,
    scratch: &mut [u8],
    stream: &mut Vec<u8>,
    now: &mut impl FnMut() -> Duration,
) -> CompressedListener {
    let mut listener = CompressedListener::new(epoch, None, false, policy, now);
    assert!(matches!(
        listener.step(epoch, scratch, now, &mut || true),
        ListenerStep::Progress
    ));
    assert!(matches!(
        listener.step(epoch, scratch, now, &mut || true),
        ListenerStep::Write { header: true, .. }
    ));
    stream.extend_from_slice(listener.write_bytes(scratch));
    assert!(matches!(
        listener.write_finished(Ok(()), now),
        ListenerStep::Progress
    ));
    listener
}

/// One frame from read to acknowledged write; for FLAC the wire number must equal frames sent.
fn deliver(
    listener: &mut CompressedListener,
    epoch: &Epoch,
    scratch: &mut [u8],
    stream: &mut Vec<u8>,
    now: &mut impl FnMut() -> Duration,
) -> u64 {
    assert!(matches!(
        listener.step(epoch, scratch, now, &mut || true),
        ListenerStep::Write { header: false, .. }
    ));
    let bytes = listener.write_bytes(scratch);
    if epoch.format == OutputFormat::Flac {
        assert_eq!(u64::from(flac_number(bytes)), listener.frames_sent);
    }
    stream.extend_from_slice(bytes);
    assert!(matches!(
        listener.write_finished(Ok(()), now),
        ListenerStep::Progress
    ));
    listener.last_sent.unwrap().sequence
}

fn decode(format: OutputFormat, stream: &[u8]) -> Vec<u8> {
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
    let body = stream.to_vec();
    let writer = std::thread::spawn(move || input.write_all(&body).unwrap());
    let output = decoder.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn frame_boundary_skip_keeps_numbering_continuous_and_the_stream_decodable() {
    for format in [OutputFormat::Flac, OutputFormat::Mp3] {
        let (_runtime, mut worker) = Runtime::new(format, false).unwrap();
        let epoch = worker.epoch.clone();
        *epoch.ring.lock().unwrap() = FlacRing::try_new(4, format.max_frame_bytes()).unwrap();
        let mut now = || Duration::from_millis(1);
        let mut scratch = vec![0; format.max_frame_bytes()];
        let mut stream = Vec::new();
        let mut listener = admit_live(
            &epoch,
            ListenerPolicy::Live,
            &mut scratch,
            &mut stream,
            &mut now,
        );
        let mut blocks = Vec::new();
        let mut expected = Vec::new();
        for number in 0..3 {
            blocks.push(publish_block(&mut worker, &epoch, number, 1000));
            let sequence = deliver(&mut listener, &epoch, &mut scratch, &mut stream, &mut now);
            expected.extend_from_slice(&blocks[sequence as usize - 1]);
        }
        // Six blocks land while the listener is away: a 4-frame ring laps it.
        for number in 3..9 {
            blocks.push(publish_block(&mut worker, &epoch, number, 1000));
        }
        let sequence = deliver(&mut listener, &epoch, &mut scratch, &mut stream, &mut now);
        assert_eq!(
            sequence, 9,
            "the newest whole frame goes out after the skip"
        );
        expected.extend_from_slice(&blocks[8]);
        assert_eq!((listener.skips, listener.frames_skipped), (1, 5));
        assert_eq!(listener.queue_frames_max, 5);
        for number in 9..11 {
            blocks.push(publish_block(&mut worker, &epoch, number, 1000));
            let sequence = deliver(&mut listener, &epoch, &mut scratch, &mut stream, &mut now);
            expected.extend_from_slice(&blocks[sequence as usize - 1]);
        }
        assert_eq!(listener.frames_sent, 6);
        let http = format.format().response_head().len();
        let decoded = decode(format, &stream[http..]);
        assert_eq!(decoded.len(), 6 * format.block_frames() * 4);
        if format == OutputFormat::Flac {
            let samples: Vec<i16> = decoded
                .chunks_exact(2)
                .map(|bytes| i16::from_le_bytes(bytes.try_into().unwrap()))
                .collect();
            assert_eq!(samples, expected);
            // Claxon checks every frame CRC, so a renumbered frame must still be whole; the
            // numbering continuity itself is what `deliver` asserts on each frame's wire bytes.
            let delivered = [1usize, 2, 3, 9, 10, 11];
            let expected: Vec<i32> = delivered
                .iter()
                .flat_map(|sequence| block_pcm(format.block_frames(), sequence - 1))
                .map(|sample| sample >> 8)
                .collect();
            let mut reader = claxon::FlacReader::new(&stream[http..]).unwrap();
            assert_eq!(
                reader.samples().collect::<Result<Vec<_>, _>>().unwrap(),
                expected
            );
        }
    }
}

#[test]
fn stale_frame_before_its_write_is_dropped_and_the_listener_moves_on() {
    for policy in [ListenerPolicy::Exact, ListenerPolicy::Live] {
        let (_runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let epoch = worker.epoch.clone();
        let mut time = Duration::ZERO;
        let mut scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
        let mut stream = Vec::new();
        let mut listener = admit_live(&epoch, policy, &mut scratch, &mut stream, &mut || time);
        // Two stale frames accepted at t=0 and one fresh frame at t=5 s, read at t=5 s.
        publish_block(&mut worker, &epoch, 0, 0);
        publish_block(&mut worker, &epoch, 1, 0);
        publish_block(&mut worker, &epoch, 2, 5_000_000);
        time = Duration::from_secs(5);
        let step = listener.step(&epoch, &mut scratch, &mut || time, &mut || true);
        match policy {
            ListenerPolicy::Exact => {
                assert!(matches!(step, ListenerStep::End(End::MediaAge)));
                assert_eq!((listener.skips, listener.frames_sent), (0, 0));
                continue;
            }
            ListenerPolicy::Live => assert!(matches!(step, ListenerStep::Progress)),
        }
        // The stale frame was never offered for writing: it counts as skipped from the moment it
        // is dropped, so a departure before the next read still accounts for it.
        assert_eq!(listener.pending_oldest_accepted_us(), None);
        assert_eq!(
            (
                listener.skips,
                listener.frames_skipped,
                listener.last_skip_us
            ),
            (1, 1, 5_000_000)
        );
        let sequence = deliver(
            &mut listener,
            &epoch,
            &mut scratch,
            &mut stream,
            &mut || time,
        );
        assert_eq!(sequence, 3);
        assert_eq!(
            (
                listener.skips,
                listener.frames_skipped,
                listener.last_skip_us
            ),
            (1, 2, 5_000_000)
        );
        assert_eq!(listener.frames_sent, 1);
        // A stale frame with nothing newer behind it: dropped, then the listener waits for supply.
        publish_block(&mut worker, &epoch, 3, 5_000_000);
        time = Duration::from_secs(10);
        assert!(matches!(
            listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
            ListenerStep::Progress
        ));
        assert!(matches!(
            listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
            ListenerStep::SleepUntil(_)
        ));
        assert_eq!((listener.skips, listener.frames_skipped), (2, 3));
        assert_eq!(listener.frames_sent, 1);
    }
}

#[test]
fn reset_during_an_owed_skip_is_terminal_and_records_nothing_more() {
    for dropped in [false, true] {
        let (_runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let epoch = worker.epoch.clone();
        let mut time = Duration::ZERO;
        let mut scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
        let mut stream = Vec::new();
        let mut listener = admit_live(
            &epoch,
            ListenerPolicy::Live,
            &mut scratch,
            &mut stream,
            &mut || time,
        );
        publish_block(&mut worker, &epoch, 0, 0);
        let counted = if dropped {
            // Stale before its write: dropped and counted now, with a move still owed.
            time = Duration::from_secs(5);
            assert!(matches!(
                listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
                ListenerStep::Progress
            ));
            (1, 1)
        } else {
            // Late after its write: nothing dropped, a move owed.
            assert!(matches!(
                listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
                ListenerStep::Write { header: false, .. }
            ));
            time = Duration::from_secs(5);
            assert!(matches!(
                listener.write_finished(Ok(()), &mut || time),
                ListenerStep::Progress
            ));
            (0, 0)
        };
        // The encoder restarts, with newer frames in the new generation, before the owed move.
        epoch.ring.lock().unwrap().reset().unwrap();
        publish_block(&mut worker, &epoch, 1, 5_000_000);
        publish_block(&mut worker, &epoch, 2, 5_000_000);
        assert!(matches!(
            listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
            ListenerStep::End(End::EncodedReset)
        ));
        assert_eq!(
            (listener.skips, listener.frames_skipped),
            counted,
            "{dropped}"
        );
    }
}

#[test]
fn retirement_network_change_and_encoder_health_precede_a_pending_skip() {
    for case in [
        "retired",
        "network",
        "unhealthy",
        "retired-lapped",
        "retired-dropped",
        "network-dropped",
        "unhealthy-dropped",
    ] {
        let network = Network::new();
        network.observe(network.snapshot(), Some("192.0.2.9".parse().unwrap()));
        let lease = network.lease().unwrap();
        let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let epoch = worker.epoch.clone();
        *epoch.ring.lock().unwrap() =
            FlacRing::try_new(2, OutputFormat::Flac.max_frame_bytes()).unwrap();
        let mut time = Duration::ZERO;
        let mut scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
        let mut stream = Vec::new();
        let mut listener = admit_live(
            &epoch,
            ListenerPolicy::Live,
            &mut scratch,
            &mut stream,
            &mut || time,
        );
        publish_block(&mut worker, &epoch, 0, 0);
        if case == "retired-lapped" {
            // The lag itself is pending: three more frames lap the two-frame ring.
            for number in 1..4 {
                publish_block(&mut worker, &epoch, number, 0);
            }
        } else if case.ends_with("-dropped") {
            // A stale frame is dropped, and counted, before the move it owes.
            time = Duration::from_secs(5);
            assert!(matches!(
                listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
                ListenerStep::Progress
            ));
            publish_block(&mut worker, &epoch, 1, 5_000_000);
        } else {
            // A late write leaves a skip owed before the next read.
            assert!(matches!(
                listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
                ListenerStep::Write { header: false, .. }
            ));
            time = Duration::from_secs(5);
            assert!(matches!(
                listener.write_finished(Ok(()), &mut || time),
                ListenerStep::Progress
            ));
            publish_block(&mut worker, &epoch, 1, 5_000_000);
            publish_block(&mut worker, &epoch, 2, 5_000_000);
        }
        let request = runtime.begin_switch(
            OutputFormat::Mp3,
            true,
            Arc::new(Mutex::new(true)),
            Duration::from_secs(1),
        );
        match case.trim_end_matches("-dropped") {
            "retired" | "retired-lapped" => assert!(worker.service(&runtime)),
            "network" => {
                network.invalidate();
            }
            "unhealthy" => assert!(runtime.health.transition(
                runtime.health.token(),
                stream_runtime::health::State::Terminal
            )),
            _ => unreachable!(),
        }
        let end = listener.step_network(
            &epoch,
            &mut scratch,
            &mut || time,
            &mut || true,
            &network,
            lease,
        );
        assert!(
            matches!(
                (case.trim_end_matches("-dropped"), &end),
                (
                    "retired" | "retired-lapped",
                    ListenerStep::End(End::Retired)
                ) | ("network", ListenerStep::End(End::NetworkChanged))
                    | ("unhealthy", ListenerStep::End(End::EncoderUnhealthy))
            ),
            "{case}"
        );
        // Only a frame already dropped is on the books; an owed move never taken counts nothing.
        let counted = if case.ends_with("-dropped") {
            (1, 1)
        } else {
            (0, 0)
        };
        assert_eq!((listener.skips, listener.frames_skipped), counted, "{case}");
        drop(request);
    }
}

#[test]
fn real_socket_reader_that_stalls_past_the_age_bound_resumes_on_the_same_socket() {
    let accept = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = std::net::TcpStream::connect(accept.local_addr().unwrap()).unwrap();
    let (socket, _) = accept.accept().unwrap();
    socket2::SockRef::from(&socket)
        .set_send_buffer_size(4096)
        .unwrap();
    socket2::SockRef::from(&peer)
        .set_recv_buffer_size(4096)
        .unwrap();
    let (_runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let epoch = worker.epoch.clone();
    *epoch.ring.lock().unwrap() =
        FlacRing::try_new(4, OutputFormat::Flac.max_frame_bytes()).unwrap();
    let mut time = Duration::ZERO;
    let mut scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
    let mut sent = Vec::new();
    let mut listener = admit_live(
        &epoch,
        ListenerPolicy::Live,
        &mut scratch,
        &mut sent,
        &mut || time,
    );
    let mut writer = DeadlineWriter::new(&socket);
    writer
        .write_all_before(&sent, Instant::now() + Duration::from_secs(1))
        .unwrap();
    // Full-scale noise makes frames large, so the stalled socket fills within a few of them.
    let noise = esp_shazam_core::signal::full_scale_noise_stereo(OutputFormat::Flac.block_frames());
    let publish = |worker: &mut Worker, at: u64| {
        for chunk in noise.chunks(480) {
            worker.encoder.push(chunk);
        }
        worker.encoder.encode_block(0, &mut worker.bytes).unwrap();
        epoch
            .ring
            .lock()
            .unwrap()
            .push(&worker.bytes, source_at(at), || at)
            .unwrap();
    };
    // The peer reads nothing for a while, then drains everything until the socket closes.
    let stall = Duration::from_millis(300);
    let reader = std::thread::spawn(move || {
        std::thread::sleep(stall);
        let mut received = Vec::new();
        peer.read_to_end(&mut received).unwrap();
        received
    });
    // Frames go out on time until the kernel buffers are full and a write blocks on the peer.
    let blocked = Instant::now();
    let mut on_time = 0;
    loop {
        assert!(on_time < 64, "the stalled peer never blocked a write");
        publish(&mut worker, 0);
        assert!(matches!(
            listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
            ListenerStep::Write { header: false, .. }
        ));
        let frame = listener.write_bytes(&scratch).to_vec();
        writer
            .write_all_before(&frame, Instant::now() + Duration::from_secs(8))
            .unwrap();
        sent.extend_from_slice(&frame);
        if blocked.elapsed() >= stall - Duration::from_millis(50) {
            break;
        }
        on_time += 1;
        assert!(matches!(
            listener.write_finished(Ok(()), &mut || time),
            ListenerStep::Progress
        ));
    }
    // On the listener's clock the blocked write outlived the 4 s media bound: late, but complete.
    time = Duration::from_secs(5);
    assert!(matches!(
        listener.write_finished(Ok(()), &mut || time),
        ListenerStep::Progress
    ));
    let delivered = u64::try_from(on_time).unwrap() + 1;
    assert_eq!(listener.frames_sent, delivered);
    // Meanwhile five more frames arrived; the listener skips to the newest and keeps going.
    for _ in 0..5 {
        publish(&mut worker, 5_000_000);
    }
    assert!(matches!(
        listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
        ListenerStep::Write { header: false, .. }
    ));
    let frame = listener.write_bytes(&scratch).to_vec();
    writer
        .write_all_before(&frame, Instant::now() + Duration::from_secs(8))
        .unwrap();
    sent.extend_from_slice(&frame);
    assert!(matches!(
        listener.write_finished(Ok(()), &mut || time),
        ListenerStep::Progress
    ));
    assert!(matches!(
        listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
        ListenerStep::SleepUntil(_)
    ));
    assert!(listener.skips >= 1);
    assert_eq!((listener.skips, listener.frames_skipped), (1, 4));
    assert_eq!(
        listener.queue_frames_max, 4,
        "the owed skip records the lag it found"
    );
    assert_eq!(
        (listener.frames_sent, listener.last_sent.unwrap().sequence),
        (delivered + 1, delivered + 5)
    );
    drop(socket);
    let received = reader.join().unwrap();
    assert_eq!(received, sent);
    let http = OutputFormat::Flac.format().response_head().len();
    let mut decoder = claxon::FlacReader::new(&received[http..]).unwrap();
    let expected: Vec<i32> = (0..delivered + 1)
        .flat_map(|_| noise.iter().map(|sample| sample >> 8))
        .collect();
    assert_eq!(
        decoder.samples().collect::<Result<Vec<_>, _>>().unwrap(),
        expected
    );
}

#[test]
fn skips_cost_no_heap_allocation() {
    use alloc_count::allocations_during;
    let (_runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
    let epoch = worker.epoch.clone();
    *epoch.ring.lock().unwrap() =
        FlacRing::try_new(4, OutputFormat::Flac.max_frame_bytes()).unwrap();
    let mut time = Duration::ZERO;
    let mut scratch = vec![0; OutputFormat::Flac.max_frame_bytes()];
    let mut stream = Vec::new();
    let mut listener = admit_live(
        &epoch,
        ListenerPolicy::Live,
        &mut scratch,
        &mut stream,
        &mut || time,
    );
    // A plain delivery: read, FLAC renumbering, and the write hand-off.
    publish_block(&mut worker, &epoch, 0, 0);
    let (step, plain) =
        allocations_during(|| listener.step(&epoch, &mut scratch, &mut || time, &mut || true));
    assert!(matches!(step, ListenerStep::Write { header: false, .. }));
    assert_eq!(plain, 0, "a plain delivery allocated");
    assert!(matches!(
        listener.write_finished(Ok(()), &mut || time),
        ListenerStep::Progress
    ));
    // Lapped: the same step also moves the cursor and counts the skip.
    for number in 1..7 {
        publish_block(&mut worker, &epoch, number, 0);
    }
    let (step, lapped) =
        allocations_during(|| listener.step(&epoch, &mut scratch, &mut || time, &mut || true));
    assert!(matches!(step, ListenerStep::Write { header: false, .. }));
    assert_eq!((listener.skips, lapped), (1, 0));
    // Late completion, then the owed skip at the next read.
    time = Duration::from_secs(5);
    let (step, late) = allocations_during(|| listener.write_finished(Ok(()), &mut || time));
    assert!(matches!(step, ListenerStep::Progress));
    assert_eq!(late, 0);
    publish_block(&mut worker, &epoch, 7, 5_000_000);
    publish_block(&mut worker, &epoch, 8, 5_000_000);
    let (step, owed) =
        allocations_during(|| listener.step(&epoch, &mut scratch, &mut || time, &mut || true));
    assert!(matches!(step, ListenerStep::Write { header: false, .. }));
    assert_eq!((listener.skips, owed), (2, 0));
    assert!(matches!(
        listener.write_finished(Ok(()), &mut || time),
        ListenerStep::Progress
    ));
    // A stale frame dropped before its write, and the skip it leaves owed.
    publish_block(&mut worker, &epoch, 9, 5_000_000);
    time = Duration::from_secs(10);
    let (step, stale) =
        allocations_during(|| listener.step(&epoch, &mut scratch, &mut || time, &mut || true));
    assert!(matches!(step, ListenerStep::Progress));
    let (step, drained) =
        allocations_during(|| listener.step(&epoch, &mut scratch, &mut || time, &mut || true));
    assert!(matches!(step, ListenerStep::SleepUntil(_)));
    assert_eq!((listener.skips, stale, drained), (3, 0, 0));
}

#[test]
fn live_frame_writes_get_the_ring_depth_and_exact_keeps_the_fixed_limit() {
    use esp_shazam_core::stream_buffer::BufferPreset;
    // An exact deadline is the smaller of the media bound and the fixed limit, so the exact rows
    // carry enough startup history for the fixed limit to be the one that shows.
    for (format, recognition, policy, preset, frame_limit_ms) in [
        (OutputFormat::Mp3, false, ListenerPolicy::Live, None, 12_000),
        (
            OutputFormat::Mp3,
            false,
            ListenerPolicy::Exact,
            Some(BufferPreset::High),
            8_016,
        ),
        (
            OutputFormat::Flac,
            false,
            ListenerPolicy::Live,
            None,
            12_032,
        ),
        (
            OutputFormat::Flac,
            false,
            ListenerPolicy::Exact,
            Some(BufferPreset::High),
            8_021,
        ),
        (OutputFormat::Flac, true, ListenerPolicy::Live, None, 2_048),
        (OutputFormat::Flac, true, ListenerPolicy::Exact, None, 2_048),
    ] {
        assert_eq!(
            format.ring_depth_ms(recognition),
            (format.history_frames(recognition) * format.block_frames()) as u64 * 1_000 / 48_000
        );
        let (_runtime, mut worker) = Runtime::new(format, recognition).unwrap();
        let epoch = worker.epoch.clone();
        let time = Duration::from_secs(1);
        let mut scratch = vec![0; format.max_frame_bytes()];
        let mut listener =
            CompressedListener::new(&epoch, preset, recognition, policy, &mut || time);
        // Buffered admission needs its history retained first; a live join reads what follows.
        for number in 0..preset.map_or(0, |preset| preset.frames(format.block_frames())) {
            publish_block(&mut worker, &epoch, number, 1_000_000);
        }
        assert!(matches!(
            listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
            ListenerStep::Progress
        ));
        // The header keeps the fixed limit under both policies.
        let ListenerStep::Write {
            deadline,
            header: true,
        } = listener.step(&epoch, &mut scratch, &mut || time, &mut || true)
        else {
            panic!("header write");
        };
        assert_eq!(
            deadline,
            time + Duration::from_millis(format.write_limit_ms(recognition)),
            "{format:?} {recognition} {policy:?}"
        );
        listener.write_finished(Ok(()), &mut || time);
        if preset.is_none() {
            publish_block(&mut worker, &epoch, 0, 1_000_000);
        }
        let step = listener.step(&epoch, &mut scratch, &mut || time, &mut || true);
        let ListenerStep::Write {
            deadline,
            header: false,
        } = step
        else {
            panic!(
                "frame write: {format:?} {recognition} {policy:?} {}",
                match step {
                    ListenerStep::End(end) => format!("{end:?}"),
                    ListenerStep::SleepUntil(at) => format!("sleep {at:?}"),
                    _ => "other".into(),
                }
            );
        };
        assert_eq!(
            deadline,
            time + Duration::from_millis(frame_limit_ms),
            "{format:?} {recognition} {policy:?}"
        );
    }
}

#[test]
fn live_mp3_listener_outlives_a_ten_second_stall_and_ends_only_past_the_ring_depth() {
    let (_runtime, mut worker) = Runtime::new(OutputFormat::Mp3, false).unwrap();
    let epoch = worker.epoch.clone();
    let mut time = Duration::ZERO;
    let mut scratch = vec![0; OutputFormat::Mp3.max_frame_bytes()];
    let mut stream = Vec::new();
    let mut listener = admit_live(
        &epoch,
        ListenerPolicy::Live,
        &mut scratch,
        &mut stream,
        &mut || time,
    );
    // The peer accepts nothing for 10 s, then takes the whole frame: inside the ring depth.
    publish_block(&mut worker, &epoch, 0, 0);
    let ListenerStep::Write { deadline, .. } =
        listener.step(&epoch, &mut scratch, &mut || time, &mut || true)
    else {
        panic!("frame write");
    };
    assert_eq!(deadline, Duration::from_secs(12));
    time = Duration::from_secs(10);
    assert!(matches!(
        listener.write_finished(Ok(()), &mut || time),
        ListenerStep::Progress
    ));
    for number in 1..3 {
        publish_block(&mut worker, &epoch, number, 10_000_000);
    }
    let sequence = deliver(
        &mut listener,
        &epoch,
        &mut scratch,
        &mut stream,
        &mut || time,
    );
    assert_eq!(sequence, 3);
    assert_eq!((listener.skips, listener.frames_skipped), (1, 1));
    assert_eq!(listener.frames_sent, 2);
    // Nothing accepted for longer than the ring holds: the socket is dead, the departure counts.
    publish_block(&mut worker, &epoch, 3, 10_000_000);
    assert!(matches!(
        listener.step(&epoch, &mut scratch, &mut || time, &mut || true),
        ListenerStep::Write { header: false, .. }
    ));
    time = Duration::from_millis(22_100);
    assert!(matches!(
        listener.write_finished(
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "ring depth"
            )),
            &mut || time
        ),
        ListenerStep::End(End::StoppedReading)
    ));
    assert_eq!((listener.frames_sent, listener.skips), (2, 1));
}
