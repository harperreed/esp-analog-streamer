#![no_main]
// ABOUTME: Exercises bounded encoder blocks and arbitrary renumber buffers.
// ABOUTME: Checks independent CRCs and byte-exact payload retention.
use esp_shazam_core::flac::{self, Encoder, MAX_FRAME_BYTES};
use esp_shazam_fuzz::{header, number};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|input: &[u8]| {
    let data = &input[..input.len().min(32784)];
    if data.len() < 16 {
        return;
    }
    let old = u32::from_le_bytes(data[1..5].try_into().unwrap()) & 0x7fffffff;
    let new = u32::from_le_bytes(data[5..9].try_into().unwrap());
    let capacity = if data[0] & 2 != 0 {
        usize::from(u16::from_le_bytes([data[12], data[13]])).min(MAX_FRAME_BYTES)
    } else {
        MAX_FRAME_BYTES
    };
    let mut buffer = vec![
        0;
        if data[0] & 1 != 0 {
            capacity
        } else {
            MAX_FRAME_BYTES
        }
    ];
    if data[0] & 1 != 0 {
        let raw = &data[16..];
        let count = raw.len().min(buffer.len());
        buffer[..count].copy_from_slice(&raw[..count]);
        let len = usize::from(u16::from_le_bytes([data[9], data[10]]));
        let before = buffer.clone();
        match flac::renumber_frame(&mut buffer, len, new) {
            Err(_) => assert_eq!(before, buffer),
            Ok(n) => {
                let end = header(&buffer[..n]);
                assert_eq!(number(&buffer[4..]), new);
                let count = if before[4] < 128 {
                    1
                } else {
                    before[4].leading_ones() as usize
                };
                assert_eq!(&before[5 + count..len - 2], &buffer[end..n - 2]);
            }
        }
    } else {
        let samples: Vec<i32> = (0..8192)
            .map(|i| {
                let bytes = &data[16..];
                if bytes.is_empty() {
                    return 0;
                }
                i32::from_le_bytes(std::array::from_fn(|j| bytes[(i * 4 + j) % bytes.len()]))
            })
            .collect();
        let mut encoder = Encoder::try_new().unwrap();
        let chunk = (usize::from(data[11]) + 1) * 2;
        for part in samples.chunks(chunk) {
            encoder.push(part);
        }
        assert_eq!(encoder.frames_wanted(), 0);
        let mut encoded = Vec::with_capacity(MAX_FRAME_BYTES);
        encoder.encode_block(old, &mut encoded);
        assert!(encoded.len() <= MAX_FRAME_BYTES);
        let end = header(&encoded);
        assert_eq!(number(&encoded[4..]), old);
        buffer[..encoded.len()].copy_from_slice(&encoded);
        match flac::renumber_frame(&mut buffer, encoded.len(), new) {
            Ok(n) => {
                let h = header(&buffer[..n]);
                assert_eq!(number(&buffer[4..]), new);
                assert_eq!(&encoded[end..encoded.len() - 2], &buffer[h..n - 2]);
            }
            Err(e) => assert_eq!(e, flac::RenumberError::NumberOutOfRange),
        }
        assert_eq!(&flac::stream_header()[..4], b"fLaC");
    }
});
