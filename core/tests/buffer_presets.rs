// ABOUTME: Verifies the public stream buffering presets and their wire values.
// ABOUTME: Keeps duration and whole-frame targets consistent across firmware and settings.
use esp_shazam_core::stream_buffer::BufferPreset;
#[test]
fn preset_values_and_default() {
    assert_eq!(BufferPreset::default(), BufferPreset::Medium);
    for (p, seconds, frames, wire) in [
        (BufferPreset::Low, 1, 12, "low"),
        (BufferPreset::Medium, 4, 47, "med"),
        (BufferPreset::High, 8, 94, "high"),
    ] {
        assert_eq!(p.seconds(), seconds);
        assert_eq!(p.frames(4096), frames);
        assert_eq!(p.query_value(), wire);
        assert_eq!(serde_json::to_string(&p).unwrap(), format!("\"{wire}\""));
        assert_eq!(
            serde_json::from_str::<BufferPreset>(&format!("\"{wire}\"")).unwrap(),
            p
        );
    }
}
#[test]
fn buffered_routes_require_a_complete_supported_compressed_query() {
    use esp_shazam_core::stream_http::{Format, Route, Status};
    assert_eq!(
        route(&format!("GET {}?buffer=med HTTP/1.1", Format::Flac.path())),
        Route::Buffered(BufferPreset::Medium)
    );
    assert_eq!(
        route(&format!("GET {} HTTP/1.1", Format::Flac.path())),
        Route::Stream(Format::Flac)
    );
    for query in [
        "buffer=",
        "buffer=medium",
        "buffer=low&x=1",
        "x=1&buffer=low",
    ] {
        assert_eq!(
            route(&format!("GET {}?{query} HTTP/1.1", Format::Flac.path())),
            Route::Refuse(Status::BadRequest)
        );
    }
    assert_eq!(
        route("GET /stream.wav?buffer=low HTTP/1.1"),
        Route::Refuse(Status::BadRequest)
    );
}
#[test]
fn retained_cursor_selects_exact_whole_frames_and_invalidates_on_reset() {
    use esp_shazam_core::flac_ring::FlacRing;
    let mut ring = FlacRing::try_new(4, 8).unwrap();
    assert!(ring.cursor_retained(3).unwrap().is_none());
    assert!(ring.cursor_retained(5).is_err());
    for i in 0..7 {
        ring.push(
            &[i],
            esp_shazam_core::flac_ring::SourceMetadata {
                raw_generation: 0,
                source_start: 0,
                source_end: 1,
                oldest_accepted_us: 0,
                raw_publication_us: 0,
            },
            || 0,
        )
        .unwrap();
    }
    let mut cursor = ring.cursor_retained(3).unwrap().unwrap();
    let mut out = [0; 8];
    for i in 4..7 {
        assert_eq!(
            ring.read(&mut cursor, &mut out).map(|read| read.bytes),
            Ok(1)
        );
        assert_eq!(out[0], i);
    }
    let mut cursor = ring.cursor_retained(3).unwrap().unwrap();
    ring.reset().unwrap();
    assert!(ring
        .read(&mut cursor, &mut out)
        .map(|read| read.bytes)
        .is_err());
    assert!(ring.cursor_retained(3).unwrap().is_none());
}
fn pair() -> (std::net::TcpStream, std::net::TcpStream) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    (listener.accept().unwrap().0, client)
}
#[test]
fn buffered_start_waits_for_history_and_detects_disconnected_peers() {
    use esp_shazam_core::{
        flac_ring::FlacRing,
        stream_wait::{buffered_cursor_before, BufferWaitError},
    };
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    let ring = Arc::new(Mutex::new(FlacRing::try_new(4, 8).unwrap()));
    let (server, client) = pair();
    let producer = ring.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        let mut ring = producer.lock().unwrap();
        for i in 0..3 {
            ring.push(
                &[i],
                esp_shazam_core::flac_ring::SourceMetadata {
                    raw_generation: 0,
                    source_start: 0,
                    source_end: 1,
                    oldest_accepted_us: 0,
                    raw_publication_us: 0,
                },
                || 0,
            )
            .unwrap();
        }
    });
    let mut cursor = buffered_cursor_before(
        &ring,
        &server,
        3,
        Instant::now() + Duration::from_millis(200),
        Duration::from_millis(100),
        Duration::from_millis(1),
    )
    .unwrap();
    let mut out = [0; 8];
    assert_eq!(
        ring.lock()
            .unwrap()
            .read(&mut cursor, &mut out)
            .map(|read| read.bytes),
        Ok(1)
    );
    assert_eq!(out[0], 0);
    worker.join().unwrap();
    ring.lock().unwrap().reset().unwrap();
    socket2::SockRef::from(&client)
        .set_linger(Some(Duration::ZERO))
        .unwrap();
    drop(client);
    let started = Instant::now();
    assert_eq!(
        buffered_cursor_before(
            &ring,
            &server,
            3,
            started + Duration::from_millis(200),
            Duration::from_millis(100),
            Duration::from_millis(1)
        ),
        Err(BufferWaitError::Disconnected)
    );
    assert!(started.elapsed() < Duration::from_millis(80));
}
#[test]
fn buffered_start_has_independent_total_and_producer_stall_budgets() {
    use esp_shazam_core::{
        flac_ring::FlacRing,
        stream_wait::{buffered_cursor_before, BufferWaitError},
    };
    use std::{
        sync::Mutex,
        time::{Duration, Instant},
    };
    let ring = Mutex::new(FlacRing::try_new(4, 8).unwrap());
    let (server, _client) = pair();
    for (total, fresh) in [(50, 200), (200, 50)] {
        let started = Instant::now();
        assert_eq!(
            buffered_cursor_before(
                &ring,
                &server,
                3,
                started + Duration::from_millis(total),
                Duration::from_millis(fresh),
                Duration::from_millis(1)
            ),
            Err(BufferWaitError::Stalled)
        );
        assert!(started.elapsed() >= Duration::from_millis(50));
        assert!(started.elapsed() < Duration::from_millis(150));
    }
}
#[test]
fn ordinary_fin_with_unread_request_bytes_remains_bounded_by_freshness() {
    use esp_shazam_core::{
        flac_ring::FlacRing,
        stream_wait::{buffered_cursor_before, BufferWaitError},
    };
    use std::{
        io::Write,
        sync::Mutex,
        time::{Duration, Instant},
    };
    let ring = Mutex::new(FlacRing::try_new(4, 8).unwrap());
    let (server, mut client) = pair();
    client.write_all(b"trailing request data").unwrap();
    client.shutdown(std::net::Shutdown::Both).unwrap();
    let started = Instant::now();
    assert_eq!(
        buffered_cursor_before(
            &ring,
            &server,
            3,
            started + Duration::from_millis(200),
            Duration::from_millis(100),
            Duration::from_millis(1)
        ),
        Err(BufferWaitError::Stalled)
    );
    assert!(started.elapsed() >= Duration::from_millis(100));
    assert!(started.elapsed() < Duration::from_millis(180));
}
#[test]
fn each_preset_decodes_exact_retained_pcm_after_ring_wrap_and_renumbering() {
    use esp_shazam_core::{flac, flac_ring::FlacRing};
    for preset in [BufferPreset::Low, BufferPreset::Medium, BufferPreset::High] {
        let mut ring = FlacRing::try_new(141, flac::MAX_FRAME_BYTES).unwrap();
        let mut encoder = flac::Encoder::new();
        let mut frame = Vec::new();
        for n in 0..150u32 {
            let pcm: Vec<i32> = (0..flac::BLOCK_FRAMES)
                .flat_map(|_| [(n as i32 + 1) * 256, -(n as i32 + 1) * 256])
                .collect();
            encoder.push(&pcm);
            encoder.encode_block(n, &mut frame);
            ring.push(
                &frame,
                esp_shazam_core::flac_ring::SourceMetadata {
                    raw_generation: 0,
                    source_start: 0,
                    source_end: 1,
                    oldest_accepted_us: 0,
                    raw_publication_us: 0,
                },
                || 0,
            )
            .unwrap();
        }
        let mut cursor = ring.cursor_retained(preset.frames(4096)).unwrap().unwrap();
        let mut stream = flac::stream_header().to_vec();
        let mut out = vec![0; flac::MAX_FRAME_BYTES];
        for n in 0..preset.frames(4096) {
            let len = ring
                .read(&mut cursor, &mut out)
                .map(|read| read.bytes)
                .unwrap();
            assert!(len > 0);
            let len = flac::renumber_frame(&mut out, len, n as u32).unwrap();
            stream.extend_from_slice(&out[..len]);
        }
        assert_eq!(
            ring.read(&mut cursor, &mut out).map(|read| read.bytes),
            Ok(0)
        );
        let expected: Vec<i32> = ((150 - preset.frames(4096))..150)
            .flat_map(|n| {
                (0..flac::BLOCK_FRAMES).flat_map(move |_| [n as i32 + 1, -(n as i32 + 1)])
            })
            .collect();
        let mut decoder = claxon::FlacReader::new(stream.as_slice()).unwrap();
        let decoded: Vec<i32> = decoder.samples().collect::<Result<_, _>>().unwrap();
        assert_eq!(decoded, expected, "claxon {}", preset.query_value());
        let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("buffer-{}.flac", preset.query_value()));
        std::fs::write(&path, &stream).unwrap();
        let result = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-f", "flac", "-i"])
            .arg(&path)
            .args(["-f", "s32le", "-"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            result.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let decoded: Vec<i32> = result
            .stdout
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()) >> 16)
            .collect();
        assert_eq!(decoded, expected, "ffmpeg {}", preset.query_value());
    }
}
#[test]
fn repeated_resets_do_not_extend_absolute_fill_budget_or_refresh_a_stalled_producer() {
    use esp_shazam_core::{
        flac_ring::FlacRing,
        stream_wait::{buffered_cursor_before, BufferWaitError},
    };
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    for push in [false, true] {
        let ring = Arc::new(Mutex::new(FlacRing::try_new(4, 8).unwrap()));
        let producer = ring.clone();
        let worker = std::thread::spawn(move || {
            for _ in 0..15 {
                std::thread::sleep(Duration::from_millis(10));
                let mut ring = producer.lock().unwrap();
                ring.reset().unwrap();
                if push {
                    ring.push(
                        &[1],
                        esp_shazam_core::flac_ring::SourceMetadata {
                            raw_generation: 0,
                            source_start: 0,
                            source_end: 1,
                            oldest_accepted_us: 0,
                            raw_publication_us: 0,
                        },
                        || 0,
                    )
                    .unwrap();
                }
            }
        });
        let (server, _client) = pair();
        let started = Instant::now();
        assert_eq!(
            buffered_cursor_before(
                &ring,
                &server,
                3,
                started + Duration::from_millis(100),
                Duration::from_millis(50),
                Duration::from_millis(1)
            ),
            Err(BufferWaitError::Stalled)
        );
        let elapsed = started.elapsed();
        worker.join().unwrap();
        assert!(elapsed >= Duration::from_millis(if push { 100 } else { 50 }));
        assert!(elapsed < Duration::from_millis(if push { 150 } else { 100 }));
    }
}
#[test]
fn held_ring_lock_does_not_hide_startup_deadline_or_peer_disconnect() {
    use esp_shazam_core::{
        flac_ring::FlacRing,
        stream_wait::{buffered_cursor_before, BufferWaitError},
    };
    use std::{
        sync::{mpsc, Arc, Mutex},
        time::{Duration, Instant},
    };
    for disconnect in [false, true] {
        let ring = Arc::new(Mutex::new(FlacRing::try_new(4, 8).unwrap()));
        let producer = ring.clone();
        let (locked, ready) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _guard = producer.lock().unwrap();
            locked.send(()).unwrap();
            released.recv().unwrap();
        });
        ready.recv().unwrap();
        let (server, client) = pair();
        if disconnect {
            socket2::SockRef::from(&client)
                .set_linger(Some(Duration::ZERO))
                .unwrap();
        }
        let _client = if disconnect {
            drop(client);
            None
        } else {
            Some(client)
        };
        let started = Instant::now();
        let result = buffered_cursor_before(
            &ring,
            &server,
            3,
            started + Duration::from_millis(50),
            Duration::from_millis(200),
            Duration::from_millis(1),
        );
        release.send(()).unwrap();
        worker.join().unwrap();
        assert_eq!(
            result,
            Err(if disconnect {
                BufferWaitError::Disconnected
            } else {
                BufferWaitError::Stalled
            })
        );
        assert!(started.elapsed() < Duration::from_millis(150));
    }
}
#[test]
fn complete_request_half_close_can_receive_buffered_response() {
    use esp_shazam_core::{
        flac_ring::FlacRing,
        stream_http::{read_request_line, DeadlineWriter, Format},
        stream_wait::buffered_cursor_before,
    };
    use std::{
        io::{BufReader, Read, Write},
        sync::Mutex,
        time::{Duration, Instant},
    };
    let ring = Mutex::new(FlacRing::try_new(4, 8).unwrap());
    let (server, mut client) = pair();
    client
        .write_all(b"GET /stream.flac?buffer=low HTTP/1.1\r\n\r\n")
        .unwrap();
    client.shutdown(std::net::Shutdown::Write).unwrap();
    assert_eq!(
        read_request_line(BufReader::new(&server), 4096).unwrap(),
        "GET /stream.flac?buffer=low HTTP/1.1"
    );
    ring.lock()
        .unwrap()
        .push(
            &[1],
            esp_shazam_core::flac_ring::SourceMetadata {
                raw_generation: 0,
                source_start: 0,
                source_end: 1,
                oldest_accepted_us: 0,
                raw_publication_us: 0,
            },
            || 0,
        )
        .unwrap();
    buffered_cursor_before(
        &ring,
        &server,
        1,
        Instant::now() + Duration::from_millis(200),
        Duration::from_millis(100),
        Duration::from_millis(1),
    )
    .unwrap();
    DeadlineWriter::new(&server)
        .write_all_before(
            Format::Flac.response_head().as_bytes(),
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
    drop(server);
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"));
}
#[test]
fn recognition_profile_rejects_buffered_routes_but_preserves_live_routes() {
    use esp_shazam_core::stream_http::{Format, Route, Status};
    assert_eq!(
        route(&format!("GET {}?buffer=low HTTP/1.1", Format::Flac.path()))
            .with_buffer_availability(false),
        Route::Refuse(Status::ServiceUnavailable)
    );
    assert_eq!(
        route(&format!("GET {} HTTP/1.1", Format::Flac.path())).with_buffer_availability(false),
        Route::Stream(Format::Flac)
    );
    assert_eq!(
        route(&format!("GET {}?buffer=low HTTP/1.1", Format::Flac.path()))
            .with_buffer_availability(true),
        Route::Buffered(BufferPreset::Low)
    );
}
#[test]
fn reset_during_fill_selects_only_restarted_history() {
    use esp_shazam_core::{flac_ring::FlacRing, stream_wait::buffered_cursor_before};
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    let ring = Arc::new(Mutex::new(FlacRing::try_new(4, 8).unwrap()));
    ring.lock()
        .unwrap()
        .push(
            b"old",
            esp_shazam_core::flac_ring::SourceMetadata {
                raw_generation: 0,
                source_start: 0,
                source_end: 1,
                oldest_accepted_us: 0,
                raw_publication_us: 0,
            },
            || 0,
        )
        .unwrap();
    let producer = ring.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        let mut ring = producer.lock().unwrap();
        ring.reset().unwrap();
        for frame in [b"one", b"two", b"end"] {
            ring.push(
                frame,
                esp_shazam_core::flac_ring::SourceMetadata {
                    raw_generation: 0,
                    source_start: 0,
                    source_end: 1,
                    oldest_accepted_us: 0,
                    raw_publication_us: 0,
                },
                || 0,
            )
            .unwrap();
        }
    });
    let (server, _client) = pair();
    let mut cursor = buffered_cursor_before(
        &ring,
        &server,
        3,
        Instant::now() + Duration::from_millis(200),
        Duration::from_millis(100),
        Duration::from_millis(1),
    )
    .unwrap();
    worker.join().unwrap();
    let mut out = [0; 8];
    for frame in [b"one", b"two", b"end"] {
        assert_eq!(
            ring.lock()
                .unwrap()
                .read(&mut cursor, &mut out)
                .map(|read| read.bytes),
            Ok(3)
        );
        assert_eq!(&out[..3], frame);
    }
    for _ in 0..5 {
        ring.lock()
            .unwrap()
            .push(
                b"lap",
                esp_shazam_core::flac_ring::SourceMetadata {
                    raw_generation: 0,
                    source_start: 0,
                    source_end: 1,
                    oldest_accepted_us: 0,
                    raw_publication_us: 0,
                },
                || 0,
            )
            .unwrap();
    }
    assert!(ring
        .lock()
        .unwrap()
        .read(&mut cursor, &mut out)
        .map(|read| read.bytes)
        .is_err());
}
#[cfg(not(feature = "listener-loom"))]
#[test]
fn rst_during_fill_releases_listener_slot_without_sending_200() {
    use esp_shazam_core::{
        flac_ring::FlacRing,
        stream_wait::{buffered_cursor_before, BufferWaitError, Slot},
    };
    use std::{
        io::{Read, Write},
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Arc, Mutex,
        },
        time::{Duration, Instant},
    };
    let ring = Arc::new(Mutex::new(FlacRing::try_new(4, 8).unwrap()));
    let slots = Arc::new(AtomicUsize::new(0));
    let slot = Slot::take(&slots, 2).unwrap();
    let (server, mut client) = pair();
    let (begun, ready) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        begun.send(()).unwrap();
        let result = buffered_cursor_before(
            &ring,
            &server,
            3,
            Instant::now() + Duration::from_millis(500),
            Duration::from_millis(300),
            Duration::from_millis(1),
        );
        drop(server);
        drop(slot);
        result
    });
    ready.recv().unwrap();
    client
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    assert!(
        client.read(&mut [0; 1]).is_err(),
        "HTTP 200 must wait for retained history"
    );
    client.write_all(b"trailing request bytes").unwrap();
    socket2::SockRef::from(&client)
        .set_linger(Some(Duration::ZERO))
        .unwrap();
    let cancelled = Instant::now();
    drop(client);
    assert_eq!(worker.join().unwrap(), Err(BufferWaitError::Disconnected));
    assert!(cancelled.elapsed() < Duration::from_millis(150));
    assert_eq!(slots.load(Ordering::Acquire), 0);
}

fn route(line: &str) -> esp_shazam_core::stream_http::Route {
    esp_shazam_core::stream_http::route_for(
        line,
        esp_shazam_core::stream_buffer::OutputFormat::Flac,
    )
}
