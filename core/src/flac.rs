// ABOUTME: A small FLAC encoder (RFC 9639) for the LINE IN stream: 48 kHz, 24-bit stereo in blocks of 4,096 frames.
// ABOUTME: Fixed predictors of order 0-4 and partitioned Rice codes, one block at a time, allocating nothing per block.

/// Frames per block: 85 ms at 48 kHz. Each FLAC frame codes one block, and every block has this
/// size, so the stream has a fixed block size.
pub const BLOCK_FRAMES: usize = 4096;
/// The stream's sample rate.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
/// Channels per frame, interleaved left then right.
pub const CHANNELS: usize = 2;
/// Bytes in `stream_header`: "fLaC", a metadata block header and the 34-byte streaminfo.
pub const STREAM_HEADER_LEN: usize = 42;
/// The most bytes one block's frame can take: the longest frame header, two verbatim subframes and
/// the CRC-16. A fixed-predictor subframe is sent only when it comes out smaller than verbatim.
pub const MAX_FRAME_BYTES: usize =
    MAX_HEADER_BYTES + (CHANNELS * VERBATIM_SUBFRAME_BITS as usize).div_ceil(8) + 2;

/// The stream ring's samples: sign-extended 24-bit values in `i32`.
const BITS_PER_SAMPLE: u32 = 24;
const SAMPLE_MAX: i32 = (1 << (BITS_PER_SAMPLE - 1)) - 1;
const SAMPLE_MIN: i32 = -(1 << (BITS_PER_SAMPLE - 1));

/// The frame header's fields (RFC 9639, section 9.1). The sync code is followed by a 0 for a fixed
/// block size, so each header counts blocks.
const SYNC_FIXED_BLOCK_SIZE: [u8; 2] = [0xFF, 0xF8];
const BLOCK_SIZE_4096: u8 = 0b1100;
const SAMPLE_RATE_48_KHZ: u8 = 0b1010;
const LEFT_RIGHT_INDEPENDENT: u8 = 0b0001;
const BITS_24: u8 = 0b110;
/// A frame number has 31 bits.
const FRAME_NUMBER_MASK: u32 = (1 << 31) - 1;
/// A frame header with the longest frame number: 4 fixed bytes, 6 bytes of coded number, the CRC-8.
const MAX_HEADER_BYTES: usize = 4 + 6 + 1;

/// A subframe's header: a 0, 6 bits of type, and a 0 for no wasted bits (RFC 9639, section 9.2.1).
const SUBFRAME_HEADER_BITS: u64 = 8;
const VERBATIM_TYPE: u32 = 0b000001;
const FIXED_TYPE: u32 = 0b001000;
/// A verbatim subframe: its header, then every sample in 24 bits.
const VERBATIM_SUBFRAME_BITS: u64 =
    SUBFRAME_HEADER_BITS + BITS_PER_SAMPLE as u64 * BLOCK_FRAMES as u64;
/// FLAC's fixed predictors go up to order 4.
const MAX_FIXED_ORDER: usize = 4;
/// The streamable subset allows up to 2^8 partitions, here 256 of 16 residuals each.
const MAX_PARTITION_ORDER: u32 = 8;
/// A residual starts with its coding method (2 bits) and partition order (4 bits).
const RESIDUAL_HEADER_BITS: u64 = 6;
/// The largest Rice parameter in a 4-bit and in a 5-bit parameter field. All ones is the escape code
/// for unencoded partitions, which this encoder never writes: a parameter up to 30 codes any residual
/// of 24-bit samples, and claxon can't read escaped partitions.
const MAX_RICE4_PARAMETER: u32 = 14;
const MAX_RICE5_PARAMETER: u32 = 30;
/// Estimates count every partition's parameter as 5 bits, so they never fall short of what gets written.
const PARAMETER_BITS_ESTIMATE: u64 = 5;

/// The stream's header: the "fLaC" marker, then a streaminfo block, the only metadata block, for an
/// endless 48 kHz, 24-bit stereo stream (RFC 9639, section 8.2). Frames from `Encoder::encode_block`
/// follow it.
pub fn stream_header() -> [u8; STREAM_HEADER_LEN] {
    let mut bytes = Vec::with_capacity(STREAM_HEADER_LEN);
    bytes.extend_from_slice(b"fLaC");
    let mut bits = BitWriter::new(&mut bytes);
    bits.put(1, 1); // the last metadata block
    bits.put(7, 0); // streaminfo
    bits.put(24, (STREAM_HEADER_LEN - 8) as u32); // its length in bytes
    bits.put(16, BLOCK_FRAMES as u32); // the minimum block size
    bits.put(16, BLOCK_FRAMES as u32); // the maximum block size
    bits.put(24, 0); // the minimum frame size: unknown
    bits.put(24, 0); // the maximum frame size: unknown
    bits.put(20, SAMPLE_RATE_HZ);
    bits.put(3, CHANNELS as u32 - 1);
    bits.put(5, BITS_PER_SAMPLE - 1);
    // Total samples, 36 bits: 0, unknown, since the stream has no end.
    bits.put(4, 0);
    bits.put(32, 0);
    // The MD5 of the audio, 128 bits: 0, unknown.
    for _ in 0..4 {
        bits.put(32, 0);
    }
    bits.finish();
    let mut header = [0; STREAM_HEADER_LEN];
    header.copy_from_slice(&bytes);
    header
}

/// Encodes blocks of the stream ring's frames as FLAC frames. It owns its working buffers, so
/// encoding a block allocates nothing: one channel of the block (16 KB), and the partition search's
/// sums and Rice parameters (2.5 KB).
pub struct Encoder {
    /// One channel of the block being encoded, clipped to 24 bits.
    channel: Vec<i32>,
    /// Folded residuals summed per partition, for the partitioning being estimated.
    sums: Vec<u64>,
    /// The Rice parameters of the partitioning being estimated, and of the cheapest one so far.
    parameters: Vec<u8>,
    best_parameters: Vec<u8>,
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    pub fn new() -> Self {
        let partitions = 1 << MAX_PARTITION_ORDER;
        Self {
            channel: vec![0; BLOCK_FRAMES],
            sums: vec![0; partitions],
            parameters: vec![0; partitions],
            best_parameters: vec![0; partitions],
        }
    }

    /// Replaces `out` with the FLAC frame for one block: `BLOCK_FRAMES` frames of interleaved
    /// sign-extended 24-bit samples, left then right, the stream ring's format. Samples beyond 24
    /// bits clip at the rails, as in `wav::pack_s24le`. `block_number` counts the blocks before this
    /// one. A frame keeps 31 bits of it, so it wraps after 2^31 blocks, 5.8 years at 48 kHz.
    ///
    /// Panics unless `interleaved` holds exactly `BLOCK_FRAMES` frames.
    pub fn encode_block(&mut self, interleaved: &[i32], block_number: u32, out: &mut Vec<u8>) {
        assert_eq!(
            interleaved.len(),
            CHANNELS * BLOCK_FRAMES,
            "a block is {BLOCK_FRAMES} frames of {CHANNELS} channels"
        );
        out.clear();
        push_frame_header(block_number & FRAME_NUMBER_MASK, out);
        let mut bits = BitWriter::new(out);
        for channel in 0..CHANNELS {
            for (sample, frame) in self
                .channel
                .iter_mut()
                .zip(interleaved.chunks_exact(CHANNELS))
            {
                *sample = frame[channel].clamp(SAMPLE_MIN, SAMPLE_MAX);
            }
            self.write_subframe(&mut bits);
        }
        bits.finish();
        let crc = crc16(out);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    /// Writes `self.channel` as the smallest subframe it can find: a fixed predictor of order 0-4
    /// with partitioned Rice codes, choosing the order and the partitioning by their estimated size,
    /// or verbatim when none comes out under 24 bits a sample.
    fn write_subframe(&mut self, bits: &mut BitWriter) {
        // (predictor order, partition order) of the smallest estimate so far; None is verbatim.
        let mut best = None;
        let mut best_bits = VERBATIM_SUBFRAME_BITS;
        for order in 0..=MAX_FIXED_ORDER {
            let warm_up_bits = order as u64 * u64::from(BITS_PER_SAMPLE);
            self.sum_finest_partitions(order);
            for partition_order in (0..=MAX_PARTITION_ORDER).rev() {
                let estimate = SUBFRAME_HEADER_BITS
                    + warm_up_bits
                    + self.estimate_residual_bits(order, partition_order);
                if estimate < best_bits {
                    best_bits = estimate;
                    best = Some((order, partition_order));
                    let partitions = 1 << partition_order;
                    self.best_parameters[..partitions]
                        .copy_from_slice(&self.parameters[..partitions]);
                }
                if partition_order > 0 {
                    self.merge_partition_pairs(partition_order);
                }
            }
        }
        match best {
            Some((order, partition_order)) => self.write_fixed(bits, order, partition_order),
            None => self.write_verbatim(bits),
        }
    }

    /// Fills `self.sums` with the folded residuals of the fixed predictor of `order`, summed over each
    /// of the finest partitions.
    fn sum_finest_partitions(&mut self, order: usize) {
        let len = BLOCK_FRAMES >> MAX_PARTITION_ORDER;
        for (j, sum) in self.sums.iter_mut().enumerate() {
            // The first partition starts after the predictor's warm-up samples.
            let start = (j * len).max(order);
            *sum = (start..(j + 1) * len)
                .map(|n| u64::from(fold(residual(&self.channel, order, n))))
                .sum();
        }
    }

    /// Turns the sums of `2^partition_order` partitions in `self.sums` into those of half as many.
    fn merge_partition_pairs(&mut self, partition_order: u32) {
        for j in 0..1 << (partition_order - 1) {
            self.sums[j] = self.sums[2 * j] + self.sums[2 * j + 1];
        }
    }

    /// The estimated bits of the residual of `order` in `2^partition_order` partitions, from their
    /// sums in `self.sums`. Each partition's Rice parameter goes into `self.parameters`.
    fn estimate_residual_bits(&mut self, order: usize, partition_order: u32) -> u64 {
        let partitions = 1 << partition_order;
        let len = (BLOCK_FRAMES >> partition_order) as u64;
        let mut bits = RESIDUAL_HEADER_BITS;
        for (j, (&sum, parameter)) in self.sums[..partitions]
            .iter()
            .zip(&mut self.parameters)
            .enumerate()
        {
            let residuals = if j == 0 { len - order as u64 } else { len };
            let (k, rice_bits) = rice_parameter(residuals, sum);
            *parameter = k as u8;
            bits += PARAMETER_BITS_ESTIMATE + rice_bits;
        }
        bits
    }

    /// Writes a fixed-predictor subframe (RFC 9639, sections 9.2.5 and 9.2.7) with the Rice
    /// parameters in `self.best_parameters`.
    fn write_fixed(&self, bits: &mut BitWriter, order: usize, partition_order: u32) {
        bits.put(8, (FIXED_TYPE | order as u32) << 1);
        for &sample in &self.channel[..order] {
            bits.put(BITS_PER_SAMPLE, sample as u32); // the warm-up samples, unencoded
        }
        let parameters = &self.best_parameters[..1 << partition_order];
        // 4-bit parameter fields (method 0b00) when every parameter fits in one, else 5-bit (0b01).
        let parameter_bits = if parameters
            .iter()
            .all(|&k| u32::from(k) <= MAX_RICE4_PARAMETER)
        {
            4
        } else {
            5
        };
        bits.put(2, parameter_bits - 4);
        bits.put(4, partition_order);
        let len = BLOCK_FRAMES >> partition_order;
        for (j, &k) in parameters.iter().enumerate() {
            bits.put(parameter_bits, u32::from(k));
            for n in (j * len).max(order)..(j + 1) * len {
                bits.put_rice(u32::from(k), fold(residual(&self.channel, order, n)));
            }
        }
    }

    /// Writes a verbatim subframe: every sample unencoded (RFC 9639, section 9.2.4).
    fn write_verbatim(&self, bits: &mut BitWriter) {
        bits.put(8, VERBATIM_TYPE << 1);
        for &sample in &self.channel {
            bits.put(BITS_PER_SAMPLE, sample as u32);
        }
    }
}

/// Appends a frame header for block `number` (RFC 9639, section 9.1), with its CRC-8. The header
/// states the block size, sample rate and bit depth itself, as the streamable subset asks, so a
/// player can start at any frame.
fn push_frame_header(number: u32, out: &mut Vec<u8>) {
    let start = out.len();
    out.extend_from_slice(&SYNC_FIXED_BLOCK_SIZE);
    out.push(BLOCK_SIZE_4096 << 4 | SAMPLE_RATE_48_KHZ);
    out.push(LEFT_RIGHT_INDEPENDENT << 4 | BITS_24 << 1); // then a reserved 0
    push_coded_number(number, out);
    let crc = crc8(&out[start..]);
    out.push(crc);
}

/// Appends `n`, at most 31 bits, as FLAC's coded number: UTF-8's scheme extended to 6 bytes (RFC
/// 9639, section 9.1.5).
fn push_coded_number(n: u32, out: &mut Vec<u8>) {
    debug_assert!(n <= FRAME_NUMBER_MASK);
    if n < 0x80 {
        out.push(n as u8);
        return;
    }
    // Each continuation byte carries 6 bits under a 0b10 prefix. The first byte carries the rest
    // under a prefix of a 1 per byte in the code, then a 0.
    let continuation_bytes: u32 = match n {
        0..0x800 => 1,
        0x800..0x1_0000 => 2,
        0x1_0000..0x20_0000 => 3,
        0x20_0000..0x400_0000 => 4,
        _ => 5,
    };
    let prefix = !(0xFF_u8 >> (continuation_bytes + 1));
    out.push(prefix | (n >> (6 * continuation_bytes)) as u8);
    for i in (0..continuation_bytes).rev() {
        out.push(0x80 | ((n >> (6 * i)) & 0x3F) as u8);
    }
}

/// The residual of the fixed predictor of `order` at sample `n` of `s`, which needs the `order`
/// samples before it (RFC 9639, section 9.2.5). For 24-bit samples it stays within 28 bits.
fn residual(s: &[i32], order: usize, n: usize) -> i32 {
    match order {
        0 => s[n],
        1 => s[n] - s[n - 1],
        2 => s[n] - 2 * s[n - 1] + s[n - 2],
        3 => s[n] - 3 * s[n - 1] + 3 * s[n - 2] - s[n - 3],
        4 => s[n] - 4 * s[n - 1] + 6 * s[n - 2] - 4 * s[n - 3] + s[n - 4],
        _ => unreachable!("FLAC's fixed predictors go up to order {MAX_FIXED_ORDER}"),
    }
}

/// A residual as Rice codes store it: 2r for r at or above 0, and -2r - 1 below (RFC 9639, section
/// 9.2.7.2).
fn fold(r: i32) -> u32 {
    ((r << 1) ^ (r >> 31)) as u32
}

/// The Rice parameter that codes `n` folded residuals summing to `sum` in the fewest bits, and that
/// number of bits, by the estimate n * (k + 1) + (sum >> k): a stop bit and k low bits per residual,
/// and the unary high parts. It never falls short, because the high parts, each residual >> k, add
/// up to at most sum >> k.
fn rice_parameter(n: u64, sum: u64) -> (u32, u64) {
    let bits = |k: u32| n * u64::from(k + 1) + (sum >> k);
    // The estimate is convex in k, so walking downhill from a guess near log2 of the mean reaches its
    // minimum. The guess is the difference of the bit lengths of the sum and the count.
    let mut k = (u64::BITS - sum.leading_zeros())
        .saturating_sub(u64::BITS - n.leading_zeros())
        .min(MAX_RICE5_PARAMETER);
    while k > 0 && bits(k - 1) <= bits(k) {
        k -= 1;
    }
    while k < MAX_RICE5_PARAMETER && bits(k + 1) < bits(k) {
        k += 1;
    }
    (k, bits(k))
}

/// Writes bits most significant first, as FLAC stores everything, and appends each byte as it fills.
struct BitWriter<'a> {
    out: &'a mut Vec<u8>,
    /// Bits not yet appended, in the low `pending` bits.
    acc: u64,
    /// At most 7 between calls.
    pending: u32,
}

impl<'a> BitWriter<'a> {
    fn new(out: &'a mut Vec<u8>) -> Self {
        Self {
            out,
            acc: 0,
            pending: 0,
        }
    }

    /// Writes the low `n` bits of `value`; `n` is at most 32. A negative sample cast to `u32` comes
    /// out as `n`-bit two's complement.
    fn put(&mut self, n: u32, value: u32) {
        debug_assert!(n <= 32);
        self.acc = (self.acc << n) | (u64::from(value) & ((1 << n) - 1));
        self.pending += n;
        while self.pending >= 8 {
            self.pending -= 8;
            self.out.push((self.acc >> self.pending) as u8);
        }
    }

    /// Writes a folded residual as a Rice code word with parameter `k`: its high part in unary (that
    /// many 0s, then a 1), then its low `k` bits.
    fn put_rice(&mut self, k: u32, folded: u32) {
        let mut zeros = folded >> k;
        while zeros >= 32 {
            self.put(32, 0);
            zeros -= 32;
        }
        self.put(zeros + 1, 1);
        self.put(k, folded);
    }

    /// Pads with 0s to a whole byte.
    fn finish(mut self) {
        if self.pending > 0 {
            self.put(8 - self.pending, 0);
        }
    }
}

/// CRC-8 with the polynomial x^8 + x^2 + x + 1, starting from 0: the frame header's check (RFC 9639,
/// section 9.1.8).
fn crc8(bytes: &[u8]) -> u8 {
    bytes
        .iter()
        .fold(0, |crc, &byte| CRC8_TABLE[usize::from(crc ^ byte)])
}

/// CRC-16 with the polynomial x^16 + x^15 + x^2 + 1, starting from 0: the whole frame's check (RFC
/// 9639, section 9.3).
fn crc16(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0, |crc, &byte| {
        (crc << 8) ^ CRC16_TABLE[usize::from((crc >> 8) as u8 ^ byte)]
    })
}

/// The CRC of each byte value, computed when the code is compiled: shift left one bit at a time, and
/// XOR in the polynomial whenever a 1 falls off the top.
static CRC8_TABLE: [u8; 256] = crc8_table();
static CRC16_TABLE: [u16; 256] = crc16_table();

const fn crc8_table() -> [u8; 256] {
    let mut table = [0; 256];
    let mut byte = 0;
    while byte < 256 {
        let mut crc = byte as u8;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
            bit += 1;
        }
        table[byte] = crc;
        byte += 1;
    }
    table
}

const fn crc16_table() -> [u16; 256] {
    let mut table = [0; 256];
    let mut byte = 0;
    while byte < 256 {
        let mut crc = (byte as u16) << 8;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
            bit += 1;
        }
        table[byte] = crc;
        byte += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{
        chord_and_chirp_stereo_48k, full_scale_noise_stereo, full_scale_square_stereo_48k,
    };

    /// The one frame of RFC 9639's example file 1 (Appendix D.1): a 6-byte header, its CRC-8, two
    /// verbatim subframes, and the frame's CRC-16.
    const RFC_EXAMPLE_FRAME: [u8; 15] = [
        0xff, 0xf8, 0x69, 0x18, 0x00, 0x00, 0xbf, 0x03, 0x58, 0xfd, 0x03, 0x12, 0x8b, 0xaa, 0x9a,
    ];

    /// The bytes of one block as 24-bit PCM, what the WAV stream sends for it.
    const PCM_BLOCK_BYTES: usize = 3 * CHANNELS * BLOCK_FRAMES;

    /// The stream header, then one frame per block of `interleaved`, numbered from 0. Every frame
    /// must fit in `MAX_FRAME_BYTES`.
    fn encode_stream(interleaved: &[i32]) -> Vec<u8> {
        let mut encoder = Encoder::new();
        let mut stream = stream_header().to_vec();
        let mut frame = vec![];
        for (n, block) in interleaved
            .chunks_exact(CHANNELS * BLOCK_FRAMES)
            .enumerate()
        {
            encoder.encode_block(block, n as u32, &mut frame);
            assert!(
                frame.len() <= MAX_FRAME_BYTES,
                "block {n}: {} bytes",
                frame.len()
            );
            stream.extend_from_slice(&frame);
        }
        stream
    }

    /// claxon's reading of `stream`: its streaminfo, and every sample, interleaved.
    fn decode_with_claxon(stream: &[u8]) -> (claxon::metadata::StreamInfo, Vec<i32>) {
        let mut reader = claxon::FlacReader::new(stream).expect("a FLAC stream header");
        let info = reader.streaminfo();
        let samples = reader
            .samples()
            .collect::<Result<Vec<i32>, _>>()
            .expect("every frame decodes");
        (info, samples)
    }

    /// Fails at the first sample that differs, rather than printing every sample.
    fn assert_same_samples(decoded: &[i32], expected: &[i32]) {
        assert_eq!(decoded.len(), expected.len(), "samples decoded");
        if let Some(i) = decoded.iter().zip(expected).position(|(a, b)| a != b) {
            panic!(
                "frame {}, channel {}: decoded {:#x}, the input was {:#x}",
                i / 2,
                i % 2,
                decoded[i],
                expected[i]
            );
        }
    }

    /// The size of each frame `encode_block` makes for the blocks of `interleaved`.
    fn frame_sizes(interleaved: &[i32]) -> Vec<usize> {
        let mut encoder = Encoder::new();
        let mut frame = vec![];
        interleaved
            .chunks_exact(CHANNELS * BLOCK_FRAMES)
            .map(|block| {
                encoder.encode_block(block, 0, &mut frame);
                frame.len()
            })
            .collect()
    }

    #[test]
    fn the_crcs_match_rfc_9639s_example_frame() {
        assert_eq!(crc8(&RFC_EXAMPLE_FRAME[..6]), 0xbf, "the header's CRC-8");
        assert_eq!(
            crc16(&RFC_EXAMPLE_FRAME[..13]),
            0xaa9a,
            "the frame's CRC-16"
        );
    }

    #[test]
    fn the_stream_header_is_streaminfo_for_48_khz_stereo_24_bit_in_4096_frame_blocks() {
        #[rustfmt::skip]
        let expected: [u8; STREAM_HEADER_LEN] = [
            b'f', b'L', b'a', b'C',
            0x80, 0x00, 0x00, 0x22,             // the last metadata block, type 0 (streaminfo), 34 bytes
            0x10, 0x00, 0x10, 0x00,             // minimum and maximum block size: 4,096 frames
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // minimum and maximum frame size: unknown
            0x0B, 0xB8,                         // 48,000 Hz: the first 16 of 20 bits
            0x03,                               // the last 4, 2 channels (stored as 1), the top bit of 24 bits (23)
            0x70,                               // the rest of 23, and the top 4 of 36 bits of total samples
            0x00, 0x00, 0x00, 0x00,             // total samples: unknown
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // MD5: unknown
        ];
        assert_eq!(stream_header(), expected);
    }

    #[test]
    fn block_numbers_are_coded_like_utf8_extended_to_31_bits() {
        let coded = |n| {
            let mut out = vec![];
            push_coded_number(n, &mut out);
            out
        };
        assert_eq!(coded(0), [0x00]);
        assert_eq!(coded(0x7F), [0x7F]);
        assert_eq!(coded(0x80), [0xC2, 0x80]);
        assert_eq!(coded(0x7FF), [0xDF, 0xBF]);
        assert_eq!(coded(0x800), [0xE0, 0xA0, 0x80]);
        assert_eq!(coded(0xFFFF), [0xEF, 0xBF, 0xBF]);
        assert_eq!(coded(0x1_0000), [0xF0, 0x90, 0x80, 0x80]);
        assert_eq!(coded(0x1F_FFFF), [0xF7, 0xBF, 0xBF, 0xBF]);
        assert_eq!(coded(0x20_0000), [0xF8, 0x88, 0x80, 0x80, 0x80]);
        assert_eq!(coded(0x3FF_FFFF), [0xFB, 0xBF, 0xBF, 0xBF, 0xBF]);
        assert_eq!(coded(0x400_0000), [0xFC, 0x84, 0x80, 0x80, 0x80, 0x80]);
        assert_eq!(coded(0x7FFF_FFFF), [0xFD, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF]);
    }

    #[test]
    fn claxon_decodes_chord_and_chirp_exactly() {
        let frames = chord_and_chirp_stereo_48k(12 * BLOCK_FRAMES);
        let (info, decoded) = decode_with_claxon(&encode_stream(&frames));
        assert_eq!(
            (info.sample_rate, info.channels, info.bits_per_sample),
            (48_000, 2, 24)
        );
        assert_eq!((info.min_block_size, info.max_block_size), (4_096, 4_096));
        assert_eq!(info.samples, None, "an endless stream has no length");
        assert_same_samples(&decoded, &frames);
    }

    #[test]
    fn claxon_decodes_silence_exactly() {
        let silence = vec![0; 3 * CHANNELS * BLOCK_FRAMES];
        assert_same_samples(&decode_with_claxon(&encode_stream(&silence)).1, &silence);
    }

    #[test]
    fn claxon_decodes_a_full_scale_square_wave_exactly() {
        let square = full_scale_square_stereo_48k(6 * BLOCK_FRAMES);
        assert_same_samples(&decode_with_claxon(&encode_stream(&square)).1, &square);
    }

    #[test]
    fn full_scale_noise_goes_out_verbatim_and_decodes_exactly() {
        let noise = full_scale_noise_stereo(3 * BLOCK_FRAMES);
        assert_same_samples(&decode_with_claxon(&encode_stream(&noise)).1, &noise);
        // No predictor beats 24 bits a sample on noise like this, so each subframe is verbatim: a
        // 1-byte subframe header and 4,096 samples of 3 bytes. Around them: the frame header's 4
        // bytes, a 1-byte block number and the CRC-8, then the CRC-16.
        let verbatim = 6 + CHANNELS * (1 + 3 * BLOCK_FRAMES) + 2;
        assert_eq!(frame_sizes(&noise), [verbatim; 3]);
    }

    #[test]
    fn the_largest_frame_is_max_frame_bytes() {
        // Verbatim subframes with the longest block number, 6 bytes coded.
        let mut frame = vec![];
        Encoder::new().encode_block(
            &full_scale_noise_stereo(BLOCK_FRAMES),
            0x7FFF_FFFF,
            &mut frame,
        );
        assert_eq!(frame.len(), MAX_FRAME_BYTES);
    }

    #[test]
    fn samples_beyond_24_bits_clip_at_the_rails() {
        let mut frames = vec![0; CHANNELS * BLOCK_FRAMES];
        frames[..8].copy_from_slice(&[
            0x80_0000,
            -0x80_0001,
            i32::MAX,
            i32::MIN,
            0x7F_FFFF,
            -0x80_0000,
            1,
            -1,
        ]);
        let (_, decoded) = decode_with_claxon(&encode_stream(&frames));
        assert_eq!(
            decoded[..8],
            [0x7F_FFFF, -0x80_0000, 0x7F_FFFF, -0x80_0000, 0x7F_FFFF, -0x80_0000, 1, -1]
        );
        assert!(decoded[8..].iter().all(|&s| s == 0));
    }

    #[test]
    fn each_frame_carries_its_block_number_in_31_bits() {
        let block = chord_and_chirp_stereo_48k(BLOCK_FRAMES);
        let numbers = [0u32, 1, 200, 0x7FFF_FFFF, 0x8000_0001];
        let mut encoder = Encoder::new();
        let mut stream = stream_header().to_vec();
        let mut frame = vec![];
        for n in numbers {
            encoder.encode_block(&block, n, &mut frame);
            stream.extend_from_slice(&frame);
        }
        let mut reader = claxon::FlacReader::new(stream.as_slice()).unwrap();
        let mut blocks = reader.blocks();
        let mut buffer = vec![];
        // A block number past 31 bits wraps: 0x8000_0001 is sent as 1.
        for expected in [0u64, 1, 200, 0x7FFF_FFFF, 1] {
            let decoded = blocks.read_next_or_eof(buffer).unwrap().expect("a frame");
            assert_eq!(decoded.time(), expected * BLOCK_FRAMES as u64);
            buffer = decoded.into_buffer();
        }
    }

    #[test]
    fn music_and_silence_take_fewer_bytes_than_pcm() {
        // A fixed predictor of order k scales a tone at f by (2 sin(pi f / 48 kHz))^k. At order 4 that
        // leaves this signal's high tones, 2.6 kHz on the left and 3.3 kHz on the right, at about 15
        // and 38 thousand: some 16 and 17 bits a sample against 24, so frames come out near 68% of PCM.
        let music = frame_sizes(&chord_and_chirp_stereo_48k(4 * BLOCK_FRAMES));
        assert!(
            music.iter().all(|&size| size * 100 < PCM_BLOCK_BYTES * 70),
            "chord and chirp frames {music:?}, against {PCM_BLOCK_BYTES} bytes of PCM"
        );
        // Silence costs a bit a sample: a Rice parameter of 0 codes each 0 as a lone stop bit.
        let silence = frame_sizes(&vec![0; 2 * CHANNELS * BLOCK_FRAMES]);
        assert!(
            silence.iter().all(|&size| size < PCM_BLOCK_BYTES / 16),
            "silent frames {silence:?}"
        );
    }

    #[test]
    #[should_panic(expected = "4096 frames")]
    fn a_block_is_4096_frames() {
        Encoder::new().encode_block(&[0; 2 * 100], 0, &mut vec![]);
    }
}
