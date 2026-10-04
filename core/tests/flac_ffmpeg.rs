// ABOUTME: Decodes the FLAC encoder's output with ffmpeg, a second decoder beside the unit tests' claxon, and compares every sample.
// ABOUTME: Needs ffmpeg on the PATH (Homebrew's on the Mac); it writes out.flac to cargo's temporary directory for integration tests.

use std::path::Path;
use std::process::Command;

use esp_shazam_core::flac::{self, BLOCK_FRAMES, CHANNELS};
use esp_shazam_core::signal::{
    chord_and_chirp_stereo_48k, full_scale_noise_stereo, full_scale_square_stereo_48k,
};

#[test]
fn ffmpeg_decodes_the_stream_to_the_same_samples() {
    // Music, silence, a full-scale square wave and noise, with changes in the middle of blocks.
    let mut frames = chord_and_chirp_stereo_48k(60_000);
    frames.extend(vec![0; 2 * 15_000]);
    frames.extend(full_scale_square_stereo_48k(20_000));
    frames.extend(full_scale_noise_stereo(10_000));
    frames.truncate(frames.len() / (CHANNELS * BLOCK_FRAMES) * CHANNELS * BLOCK_FRAMES);

    let mut stream = flac::stream_header().to_vec();
    let mut encoder = flac::Encoder::new();
    let mut frame = vec![];
    for (n, block) in frames.chunks_exact(CHANNELS * BLOCK_FRAMES).enumerate() {
        encoder.encode_block(block, n as u32, &mut frame);
        stream.extend_from_slice(&frame);
    }
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("out.flac");
    std::fs::write(&path, &stream).expect("out.flac written");

    let ffmpeg = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(&path)
        .args(["-f", "s32le", "-"])
        .output()
        .expect("ffmpeg runs");
    let errors = String::from_utf8_lossy(&ffmpeg.stderr);
    assert!(ffmpeg.status.success(), "ffmpeg failed: {errors}");
    assert!(errors.is_empty(), "ffmpeg reported: {errors}");
    // ffmpeg decodes 24-bit FLAC into the top 24 bits of 32-bit samples.
    let decoded: Vec<i32> = ffmpeg
        .stdout
        .chunks_exact(4)
        .map(|bytes| i32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    let expected: Vec<i32> = frames.iter().map(|&sample| sample << 8).collect();
    assert_eq!(decoded.len(), expected.len(), "samples decoded");
    if let Some(i) = decoded.iter().zip(&expected).position(|(a, b)| a != b) {
        panic!(
            "frame {}, channel {}: ffmpeg decoded {:#x}, the input was {:#x}",
            i / 2,
            i % 2,
            decoded[i],
            expected[i]
        );
    }
}
