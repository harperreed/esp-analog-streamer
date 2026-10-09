// ABOUTME: Measures the actual production core FLAC encoder on private feasibility fixtures.
// ABOUTME: Converts int16 fixtures to capture units and writes only complete FLAC blocks.
use esp_shazam_core::flac::{self, Encoder};
use std::{env, fs, io::Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 3 { return Err("usage: flac-probe input.s16le output.flac".into()); }
    let raw = fs::read(&args[1])?;
    if raw.len() % 4 != 0 { return Err("input must contain whole stereo int16 frames".into()); }
    let blocks = raw.len() / (flac::BLOCK_FRAMES * 4);
    let mut file = fs::File::create(&args[2])?;
    file.write_all(&flac::stream_header())?;
    let mut encoder = Encoder::new();
    let mut frame = Vec::with_capacity(flac::MAX_FRAME_BYTES);
    let mut bytes = 0;
    for (number, chunk) in raw.chunks_exact(flac::BLOCK_FRAMES * 4).enumerate() {
        let samples: Vec<i32> = chunk.chunks_exact(2).map(|b| i32::from(i16::from_le_bytes([b[0], b[1]])) << 8).collect();
        encoder.push(&samples);
        encoder.encode_block(number as u32, &mut frame);
        file.write_all(&frame)?;
        bytes += frame.len();
    }
    println!("{{\"blocks\":{blocks},\"samples_per_channel\":{},\"frame_bytes\":{bytes},\"stream_header_bytes\":{}}}", blocks*flac::BLOCK_FRAMES, flac::STREAM_HEADER_LEN);
    Ok(())
}
