// ABOUTME: Checks adaptive stereo frames against Claxon and FFmpeg using exact quantized PCM.
// ABOUTME: Covers reversible side arithmetic, supported assignments, CRCs, and listener numbering.

use esp_shazam_core::flac::{self, BLOCK_FRAMES, CHANNELS, MAX_FRAME_BYTES};
use std::process::Command;

fn noise(state: &mut u32) -> i32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as i16 as i32) << 8
}

fn cases() -> Vec<(&'static str, Vec<i32>)> {
    let mut seed = 0x45_0009;
    let independent: Vec<_> = (0..BLOCK_FRAMES * CHANNELS)
        .map(|_| noise(&mut seed))
        .collect();
    let mut result = Vec::new();
    for name in [
        "identical",
        "opposite",
        "left_only",
        "right_only",
        "extremes",
        "correlated",
        "side_extremes",
    ] {
        let mut pcm = Vec::new();
        for i in 0..BLOCK_FRAMES {
            let left = independent[2 * i];
            let pair = match name {
                "identical" => [left, left],
                "opposite" => [left, -left],
                "left_only" => [left, 0],
                "right_only" => [0, left],
                "extremes" => [
                    [i32::MIN, i32::MAX],
                    [i32::MAX, i32::MIN],
                    [i32::MIN, i32::MIN],
                    [i32::MAX, i32::MAX],
                ][i % 4],
                "correlated" => [left, left.saturating_add(((i % 7) as i32 - 3) * 256)],
                "side_extremes" => {
                    let positive = (left >> 8) & 0x7fff;
                    [positive << 8, (positive - 32768) << 8]
                }
                _ => unreachable!(),
            };
            pcm.extend_from_slice(&pair);
        }
        result.push((name, pcm));
    }
    result.push(("independent", independent));
    result
}

fn encode(pcm: &[i32], number: u32) -> Vec<u8> {
    let mut encoder = flac::Encoder::new();
    for chunk in pcm.chunks(240 * CHANNELS) {
        encoder.push(chunk);
    }
    let mut frame = Vec::new();
    encoder.encode_block(number, &mut frame);
    frame
}

fn crc(bytes: &[u8], width: u32, polynomial: u32) -> u32 {
    let mut value = 0;
    for &byte in bytes {
        value ^= u32::from(byte) << (width - 8);
        for _ in 0..8 {
            value = (value << 1)
                ^ if value & (1 << (width - 1)) != 0 {
                    polynomial
                } else {
                    0
                };
        }
        value &= (1 << width) - 1;
    }
    value
}

fn assert_frame(frame: &[u8]) {
    assert!(matches!(frame[3] >> 4, 1 | 8), "unsupported assignment");
    assert_eq!(MAX_FRAME_BYTES, 16_399);
    assert!(frame.len() <= MAX_FRAME_BYTES);
    let count = (frame[4].leading_ones() as usize).max(1);
    assert_eq!(crc(&frame[..5 + count], 8, 0x07), 0, "header CRC");
    assert_eq!(crc(frame, 16, 0x8005), 0, "frame CRC");
}

#[test]
fn identical_varying_channels_select_left_side() {
    let (_, pcm) = cases().remove(0);
    let frame = encode(&pcm, 0);
    assert_eq!(
        frame[3] >> 4,
        8,
        "identical random channels benefit from left/side"
    );
    assert!(frame.len() < 9000, "one random channel plus constant side");
}

#[test]
fn stereo_cases_decode_exactly_before_and_after_number_width_changes() {
    for (name, pcm) in cases() {
        let original = encode(&pcm, 0);
        assert_frame(&original);
        if name == "side_extremes" {
            assert_eq!(original[3] >> 4, 8, "constant positive side needs 17 bits");
        }
        if name == "independent" {
            assert_eq!(original[3] >> 4, 1, "incompressible independent fallback");
            let widest = encode(&pcm, 0x7fff_ffff);
            assert_eq!(
                widest.len(),
                16_399,
                "verbatim reaches the allocation bound"
            );
            assert_frame(&widest);
        }
        let expected: Vec<_> = pcm
            .iter()
            .map(|&sample| (sample >> 8).clamp(-32768, 32767))
            .collect();
        let mut buffer = vec![0; MAX_FRAME_BYTES];
        buffer[..original.len()].copy_from_slice(&original);
        let mut len = original.len();
        for number in [0, 128, 2048, 65536, 0x20_0000, 0x400_0000, 0x7fff_ffff, 1] {
            len = flac::renumber_frame(&mut buffer, len, number).unwrap();
            let frame = &buffer[..len];
            assert_frame(frame);
            assert_eq!(frame[3], original[3], "assignment survives numbering");
            let mut stream = flac::stream_header().to_vec();
            stream.extend_from_slice(frame);
            let mut reader = claxon::FlacReader::new(stream.as_slice()).unwrap();
            let decoded: Vec<_> = reader.samples().collect::<Result<_, _>>().unwrap();
            assert_eq!(decoded, expected, "Claxon {name}, number {number}");
            let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                .join(format!("stereo-{name}-{number}.flac"));
            std::fs::write(&path, &stream).unwrap();
            let output = Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(&path)
                .args(["-f", "s16le", "-"])
                .output()
                .expect("FFmpeg runs");
            assert!(
                output.status.success(),
                "FFmpeg {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                output.stderr.is_empty(),
                "FFmpeg reports errors: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout.len() % 2, 0);
            let decoded: Vec<_> = output
                .stdout
                .chunks_exact(2)
                .map(|b| i32::from(i16::from_le_bytes(b.try_into().unwrap())))
                .collect();
            assert_eq!(decoded, expected, "FFmpeg {name}, number {number}");
        }
    }
}
