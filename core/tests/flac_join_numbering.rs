// ABOUTME: Verifies listener-local FLAC numbering independently of tolerant audio decoders.
// ABOUTME: Checks header/footer CRCs, unchanged subframes, bounds, and bit-exact decoded PCM.

use esp_shazam_core::flac::{self, BLOCK_FRAMES, CHANNELS, MAX_FRAME_BYTES};
use esp_shazam_core::signal::{chord_and_chirp_stereo_48k, full_scale_noise_stereo};
use std::path::Path;
use std::process::Command;

// Independent bit-at-a-time checks, separate from the encoder's lookup tables.
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

fn number_and_header_len(frame: &[u8]) -> (u32, usize) {
    assert_eq!(&frame[..4], &[0xff, 0xf8, 0xca, 0x18]);
    let first = frame[4];
    let count = first.leading_ones() as usize;
    if count == 0 {
        return (u32::from(first), 6);
    }
    assert!((2..=6).contains(&count));
    let mut number = u32::from(first & (0x7f >> count));
    for &byte in &frame[5..4 + count] {
        assert_eq!(byte & 0xc0, 0x80);
        number = (number << 6) | u32::from(byte & 0x3f);
    }
    (number, 5 + count)
}

fn assert_crcs(frame: &[u8]) {
    let (_, header) = number_and_header_len(frame);
    assert_eq!(crc(&frame[..header], 8, 0x07), 0, "header CRC");
    assert_eq!(crc(frame, 16, 0x8005), 0, "frame CRC");
}

fn constant_frame(number: u32) -> Vec<u8> {
    let mut encoder = flac::Encoder::new();
    encoder.push(&vec![42; BLOCK_FRAMES * CHANNELS]);
    let mut frame = Vec::new();
    encoder.encode_block(number, &mut frame);
    frame
}

#[test]
fn number_width_changes_preserve_payload_and_rebuild_crcs() {
    let numbers = [
        0,
        1,
        127,
        128,
        2047,
        2048,
        16384,
        65535,
        65536,
        0x1f_ffff,
        0x20_0000,
        0x3ff_ffff,
        0x400_0000,
        0x7fff_ffff,
    ];
    for old in numbers {
        let original = constant_frame(old);
        let (_, old_header) = number_and_header_len(&original);
        for new in numbers {
            let mut buffer = [0xa5; MAX_FRAME_BYTES];
            buffer[..original.len()].copy_from_slice(&original);
            let len = flac::renumber_frame(&mut buffer, original.len(), new).unwrap();
            let (number, header) = number_and_header_len(&buffer[..len]);
            assert_eq!(number, new);
            assert_eq!(len + old_header, original.len() + header);
            assert_eq!(
                &buffer[header..len - 2],
                &original[old_header..original.len() - 2]
            );
            assert_crcs(&buffer[..len]);
            assert!(buffer[len.max(original.len())..].iter().all(|&b| b == 0xa5));
            if old == new {
                assert_eq!(&buffer[..len], original);
            }
        }
    }
    // A number-only edit invalidates both old CRCs; fixing just CRC-8 is insufficient.
    let mut frame = constant_frame(1);
    frame[4] = 0;
    assert_ne!(crc(&frame[..6], 8, 0x07), 0);
    assert_ne!(crc(&frame, 16, 0x8005), 0);
    frame[5] = crc(&frame[..5], 8, 0x07) as u8;
    assert_ne!(crc(&frame, 16, 0x8005), 0);
}

fn assert_rejected(mut buffer: Vec<u8>, len: usize, number: u32, error: flac::RenumberError) {
    let before = buffer.clone();
    assert_eq!(flac::renumber_frame(&mut buffer, len, number), Err(error));
    assert_eq!(buffer, before, "errors must not partially rewrite a frame");
}

#[test]
fn bounds_and_number_limit_are_explicit() {
    use flac::RenumberError::*;
    let frame = constant_frame(127);
    let len = frame.len();
    assert_rejected(frame.clone(), len + 1, 0, InvalidLength);
    assert_rejected(frame.clone(), usize::MAX, 0, InvalidLength);
    assert_rejected(
        vec![0; MAX_FRAME_BYTES + 1],
        MAX_FRAME_BYTES + 1,
        0,
        InvalidLength,
    );
    assert_rejected(frame.clone(), len, 128, InsufficientCapacity);
    assert_rejected(frame.clone(), len, 0x8000_0000, NumberOutOfRange);
    assert_rejected(frame.clone(), len, u32::MAX, NumberOutOfRange);
    let mut exact = frame;
    exact.push(0);
    assert_eq!(flac::renumber_frame(&mut exact, len, 128), Ok(len + 1));
    assert_crcs(&exact);
    assert_eq!(flac::renumber_frame(&mut exact, len + 1, 0), Ok(len));
    assert_crcs(&exact[..len]);

    // Verbatim stereo occupies the worst-case payload and must still fit with six number bytes.
    let mut encoder = flac::Encoder::new();
    encoder.push(&full_scale_noise_stereo(BLOCK_FRAMES));
    let mut encoded = Vec::new();
    encoder.encode_block(0, &mut encoded);
    let maximum = 11 + 2 * (1 + 2 * BLOCK_FRAMES) + 2;
    assert_eq!(encoded.len() + 5, maximum);
    let mut buffer = [0; MAX_FRAME_BYTES];
    buffer[..encoded.len()].copy_from_slice(&encoded);
    assert_eq!(
        flac::renumber_frame(&mut buffer, encoded.len(), 0x7fff_ffff),
        Ok(maximum)
    );
    assert_crcs(&buffer[..maximum]);
}

#[test]
fn malformed_and_truncated_headers_are_rejected_without_mutation() {
    use flac::RenumberError::*;
    for number in [0, 128, 2048, 65536, 0x20_0000, 0x400_0000] {
        let frame = constant_frame(number);
        for len in 0..frame.len() {
            assert_rejected(frame[..len].to_vec(), len, 0, InvalidLength);
        }
        let (_, header) = number_and_header_len(&frame);
        for index in 0..4 {
            for bit in 0..8 {
                let mut bad = frame.clone();
                bad[index] ^= 1 << bit;
                // A valid checksum must not admit another FLAC header form.
                bad[header - 1] = crc(&bad[..header - 1], 8, 0x07) as u8;
                assert_rejected(bad, frame.len(), 0, InvalidHeader);
            }
        }
        let mut bad = frame.clone();
        bad[header - 1] ^= 1;
        assert_rejected(bad, frame.len(), 0, InvalidHeader);
    }
    for coded in [
        vec![0x80],
        vec![0xff],
        vec![0xfe],
        vec![0xc0, 0x80],
        vec![0xe0, 0x81, 0xbf],
        vec![0xc2, 0x7f],
        vec![0xfc, 0x80, 0x80, 0x80, 0x80, 0x80],
    ] {
        let frame = constant_frame(0);
        let mut bad = frame[..4].to_vec();
        bad.extend(coded);
        bad.push(crc(&bad, 8, 0x07) as u8);
        bad.extend_from_slice(&frame[6..]);
        let len = bad.len();
        assert_rejected(bad, len, 0, InvalidHeader);
    }
}

#[test]
fn joined_frames_start_at_zero_and_decode_bit_exact() {
    for first in [1, 127, 128, 16_384] {
        let mut encoder = flac::Encoder::new();
        let mut expected = chord_and_chirp_stereo_48k(BLOCK_FRAMES);
        expected.extend(full_scale_noise_stereo(BLOCK_FRAMES));
        expected.extend(vec![0; BLOCK_FRAMES * CHANNELS]);
        let mut stream = flac::stream_header().to_vec();
        let mut encoded = Vec::new();
        let mut buffer = [0; MAX_FRAME_BYTES];
        for (local, pcm) in expected.chunks_exact(BLOCK_FRAMES * CHANNELS).enumerate() {
            encoder.push(pcm);
            encoder.encode_block(first + local as u32, &mut encoded);
            assert_crcs(&encoded);
            let (original, old_header) = number_and_header_len(&encoded);
            assert_eq!(original, first + local as u32);
            buffer[..encoded.len()].copy_from_slice(&encoded);
            let len = flac::renumber_frame(&mut buffer, encoded.len(), local as u32).unwrap();
            let (number, header) = number_and_header_len(&buffer[..len]);
            assert_eq!(number, local as u32, "join at shared frame {first}");
            assert_crcs(&buffer[..len]);
            assert_eq!(
                &buffer[header..len - 2],
                &encoded[old_header..encoded.len() - 2]
            );
            stream.extend_from_slice(&buffer[..len]);
        }
        let expected: Vec<_> = expected
            .into_iter()
            .map(|s| s.clamp(-0x80_0000, 0x7f_ffff) >> 8)
            .collect();
        let mut reader = claxon::FlacReader::new(stream.as_slice()).unwrap();
        assert_eq!(
            reader.samples().collect::<Result<Vec<_>, _>>().unwrap(),
            expected
        );
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "flac-numbering-{}-{first}.flac",
            std::process::id()
        ));
        std::fs::write(&path, &stream).unwrap();
        let output = Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(&path)
            .args(["-f", "s32le", "-"])
            .output()
            .expect("ffmpeg runs");
        std::fs::remove_file(path).unwrap();
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
        assert_eq!(output.stdout.len() % 4, 0);
        let decoded: Vec<_> = output
            .stdout
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()) >> 16)
            .collect();
        assert_eq!(decoded, expected);
    }
}
