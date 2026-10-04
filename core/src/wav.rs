// ABOUTME: The endless WAV the stream server sends: a 44-byte PCM header whose sizes mean "until the connection ends",
// ABOUTME: and the packing of sign-extended 24-bit samples into the 3-byte little-endian words that follow it.

/// Bytes in the header `endless_header_s24le` makes.
pub const HEADER_LEN: usize = 44;

/// Bytes per packed sample.
const SAMPLE_BYTES: u16 = 3;
/// The range of a 24-bit sample.
const SAMPLE_MAX: i32 = (1 << 23) - 1;
const SAMPLE_MIN: i32 = -(1 << 23);

/// The canonical 44-byte header for 24-bit PCM at `sample_rate_hz` with `channels` interleaved
/// channels, for a stream with no known end: the RIFF and data sizes are 0xFFFFFFFF, the most a
/// WAV can say. ffmpeg reads that data size as "until the stream ends". It isn't a whole number of
/// stereo frames (6 bytes), so a strict reader such as hound refuses it.
pub fn endless_header_s24le(sample_rate_hz: u32, channels: u16) -> [u8; HEADER_LEN] {
    let frame_bytes = channels * SAMPLE_BYTES;
    let mut header = [0u8; HEADER_LEN];
    let fields: [(usize, &[u8]); 13] = [
        (0, b"RIFF"),
        (4, &u32::MAX.to_le_bytes()),
        (8, b"WAVE"),
        (12, b"fmt "),
        (16, &16u32.to_le_bytes()), // the fmt chunk's size
        (20, &1u16.to_le_bytes()),  // format 1: integer PCM
        (22, &channels.to_le_bytes()),
        (24, &sample_rate_hz.to_le_bytes()),
        (28, &(sample_rate_hz * u32::from(frame_bytes)).to_le_bytes()),
        (32, &frame_bytes.to_le_bytes()),
        (34, &(8 * SAMPLE_BYTES).to_le_bytes()),
        (36, b"data"),
        (40, &u32::MAX.to_le_bytes()),
    ];
    for (at, bytes) in fields {
        header[at..at + bytes.len()].copy_from_slice(bytes);
    }
    header
}

/// Replaces `out` with `samples` as 24-bit little-endian PCM, three bytes each, the format the
/// header announces. Samples are sign-extended 24-bit values in `i32` (the `StreamRing` contract).
/// Anything outside that range clips at the 24-bit rails rather than wrapping to the other sign.
pub fn pack_s24le(samples: &[i32], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(samples.len() * usize::from(SAMPLE_BYTES));
    for &sample in samples {
        let bytes = sample.clamp(SAMPLE_MIN, SAMPLE_MAX).to_le_bytes();
        out.extend_from_slice(&bytes[..usize::from(SAMPLE_BYTES)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_announces_48_khz_stereo_24_bit_pcm_without_an_end() {
        #[rustfmt::skip]
        let expected: [u8; HEADER_LEN] = [
            b'R', b'I', b'F', b'F', 0xFF, 0xFF, 0xFF, 0xFF, b'W', b'A', b'V', b'E',
            b'f', b'm', b't', b' ', 16, 0, 0, 0, // a 16-byte fmt chunk
            1, 0,                   // PCM
            2, 0,                   // channels
            0x80, 0xBB, 0x00, 0x00, // 48,000 frames per second
            0x00, 0x65, 0x04, 0x00, // 288,000 bytes per second
            6, 0,                   // bytes per frame
            24, 0,                  // bits per sample
            b'd', b'a', b't', b'a', 0xFF, 0xFF, 0xFF, 0xFF,
        ];
        assert_eq!(endless_header_s24le(48_000, 2), expected);
    }

    #[test]
    fn the_rate_and_channel_count_set_the_byte_rate_and_frame_size() {
        let header = endless_header_s24le(44_100, 1);
        assert_eq!(header[22..24], 1u16.to_le_bytes(), "channels");
        assert_eq!(header[24..28], 44_100u32.to_le_bytes(), "sample rate");
        assert_eq!(header[28..32], 132_300u32.to_le_bytes(), "bytes per second");
        assert_eq!(header[32..34], 3u16.to_le_bytes(), "bytes per frame");
        assert_eq!(header[34..36], 24u16.to_le_bytes(), "bits per sample");
    }

    #[test]
    fn each_sample_becomes_three_little_endian_bytes() {
        let mut out = vec![];
        pack_s24le(
            &[0, 1, -1, 0x12_3456, -0x12_3456, 0x7F_FFFF, -0x80_0000],
            &mut out,
        );
        #[rustfmt::skip]
        let expected = [
            0x00, 0x00, 0x00,
            0x01, 0x00, 0x00,
            0xFF, 0xFF, 0xFF,
            0x56, 0x34, 0x12,
            0xAA, 0xCB, 0xED, // -0x123456 is 0xEDCBAA in 24-bit two's complement
            0xFF, 0xFF, 0x7F, // the largest 24-bit sample
            0x00, 0x00, 0x80, // the smallest
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn samples_beyond_24_bits_clip_at_the_rails_instead_of_wrapping() {
        let mut out = vec![];
        pack_s24le(&[0x80_0000, i32::MAX, -0x80_0001, i32::MIN], &mut out);
        #[rustfmt::skip]
        let expected = [
            0xFF, 0xFF, 0x7F,
            0xFF, 0xFF, 0x7F,
            0x00, 0x00, 0x80,
            0x00, 0x00, 0x80,
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn packing_replaces_what_out_held() {
        let mut out = vec![9; 10];
        pack_s24le(&[1], &mut out);
        assert_eq!(out, [1, 0, 0]);
    }

    #[test]
    fn hound_decodes_the_header_and_packed_frames_back_to_the_samples() {
        // Left climbs from the bottom rail and right falls from the top one, so a swapped channel,
        // a lost sign or a byte out of place shows.
        let frames: Vec<i32> = (0..1_000)
            .flat_map(|i| [-0x80_0000 + i * 16_769, 0x7F_FFFF - i * 16_769])
            .collect();
        let mut data = vec![];
        pack_s24le(&frames, &mut data);
        // hound refuses a data size of 0xFFFFFFFF, which is no whole number of 6-byte frames, so the
        // test writes the real sizes in. Everything else is what the stream sends.
        let mut wav = endless_header_s24le(48_000, 2).to_vec();
        let data_len = u32::try_from(data.len()).unwrap();
        wav[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
        wav[40..44].copy_from_slice(&data_len.to_le_bytes());
        wav.extend_from_slice(&data);

        let mut reader = hound::WavReader::new(wav.as_slice()).expect("a valid WAV");
        assert_eq!(
            reader.spec(),
            hound::WavSpec {
                channels: 2,
                sample_rate: 48_000,
                bits_per_sample: 24,
                sample_format: hound::SampleFormat::Int,
            }
        );
        let decoded: Vec<i32> = reader
            .samples::<i32>()
            .collect::<Result<_, _>>()
            .expect("samples decode");
        assert_eq!(decoded, frames);
    }
}
