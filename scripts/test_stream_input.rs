// ABOUTME: Generates channel-distinct constant blocks with the real production FLAC encoder.
// ABOUTME: Feeds paced loopback decoder tests without firmware, hardware or copied codec logic.
#[allow(dead_code)]
#[path = "../core/src/flac.rs"]
mod flac;

use std::io::{self, Write};

fn main() {
    let mut out = io::stdout().lock();
    out.write_all(&flac::stream_header()).unwrap();
    let mut encoder = flac::Encoder::new();
    let samples: Vec<_> = (0..flac::BLOCK_FRAMES)
        .flat_map(|_| [100 * 256, -200 * 256])
        .collect();
    for number in 0..96 {
        encoder.push(&samples);
        let mut frame = Vec::new();
        encoder.encode_block(number, &mut frame);
        out.write_all(&(frame.len() as u32).to_le_bytes()).unwrap();
        out.write_all(&frame).unwrap();
    }
}
