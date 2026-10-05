// ABOUTME: Checks a listener joining encoded FLAC at a nonzero frame number with claxon and ffmpeg.
// ABOUTME: Frames copied in small slices must decode to the exact source samples after a fresh stream header.

use std::path::Path;
use std::process::Command;

use esp_shazam_core::flac::{self, BLOCK_FRAMES, CHANNELS, MAX_FRAME_BYTES};
use esp_shazam_core::flac_ring::FlacRing;
use esp_shazam_core::signal::{chord_and_chirp_stereo_48k, full_scale_noise_stereo};

#[test]
fn joined_frames_decode_bit_exact_with_both_decoders() {
    for first in [1, 127, 128, 16_384] {
        let mut encoder = flac::Encoder::new();
        let mut encoded = Vec::new();
        let mut ring = FlacRing::try_new(2, MAX_FRAME_BYTES).unwrap();
        encoder.push(&vec![0; BLOCK_FRAMES * CHANNELS]);
        encoder.encode_block(first - 1, &mut encoded);
        ring.push(&encoded).unwrap();
        let mut cursor = ring.cursor_now();
        let mut expected = chord_and_chirp_stereo_48k(BLOCK_FRAMES);
        expected.extend(full_scale_noise_stereo(BLOCK_FRAMES));
        for (i, block) in expected.chunks_exact(BLOCK_FRAMES * CHANNELS).enumerate() {
            encoder.push(block);
            encoder.encode_block(first + i as u32, &mut encoded);
            ring.push(&encoded).unwrap();
        }
        // Exactly two frames behind, with the old frame's slot already overwritten.
        let mut stream = flac::stream_header().to_vec();
        let mut out = [0; 113];
        loop {
            let n = ring.read(&mut cursor, &mut out).unwrap();
            if n == 0 {
                break;
            }
            stream.extend_from_slice(&out[..n]);
            assert!(stream.len() <= flac::STREAM_HEADER_LEN + 2 * MAX_FRAME_BYTES);
        }
        let mut reader = claxon::FlacReader::new(stream.as_slice()).unwrap();
        let decoded = reader.samples().collect::<Result<Vec<_>, _>>().unwrap();
        assert_samples(&decoded, &expected, "claxon", first);

        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "flac-ring-join-{}-{first}.flac",
            std::process::id()
        ));
        std::fs::write(&path, &stream).unwrap();
        let ffmpeg = Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(&path)
            .args(["-f", "s32le", "-"])
            .output()
            .expect("ffmpeg runs");
        let errors = String::from_utf8_lossy(&ffmpeg.stderr);
        assert!(ffmpeg.status.success(), "ffmpeg failed: {errors}");
        assert!(errors.is_empty(), "ffmpeg reported: {errors}");
        assert_eq!(ffmpeg.stdout.len() % 4, 0);
        let decoded: Vec<_> = ffmpeg
            .stdout
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()) >> 8)
            .collect();
        assert_samples(&decoded, &expected, "ffmpeg", first);
        std::fs::remove_file(&path).unwrap();
    }
}

fn assert_samples(decoded: &[i32], expected: &[i32], decoder: &str, first: u32) {
    assert_eq!(
        decoded.len(),
        expected.len(),
        "{decoder} sample count, first frame {first}"
    );
    if let Some(i) = decoded.iter().zip(expected).position(|(a, b)| a != b) {
        panic!(
            "{decoder} first frame {first}, sample {i}: {} != {}",
            decoded[i], expected[i]
        );
    }
}
