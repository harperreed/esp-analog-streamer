// ABOUTME: A small FLAC encoder (RFC 9639) for the LINE IN stream: 48 kHz, 24-bit stereo in blocks of 4,096 frames.
// ABOUTME: Constant subframes, fixed predictors of order 0-4 and partitioned Rice codes, allocating nothing per block.

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
/// A sample's 24 bits of two's complement.
const SAMPLE_MASK: u32 = (1 << BITS_PER_SAMPLE) - 1;

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
/// Up to 2^5 partitions, 32 of 128 residuals each: libFLAC's default limit (the streamable subset
/// allows 2^8). On 17 s of the board's music, finer partitions saved nothing: frames came out at
/// 63.2% of PCM with limits of 5 through 8.
const MAX_PARTITION_ORDER: u32 = 5;
const PARTITIONS: usize = 1 << MAX_PARTITION_ORDER;
const PARTITION_FRAMES: usize = BLOCK_FRAMES / PARTITIONS;
/// `push` sums folded residuals in a `u32` over this many frames at a time: 16 of at most 28 bits
/// fit. It adds each stretch into the partition's `u64` sum.
const SUM_FRAMES: usize = 16;
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
    let mut bits = BitWriter::new(&mut bytes, 0);
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

/// Builds blocks of the stream ring's frames and encodes each as a FLAC frame. Frames come in with
/// `push`, a ring read at a time, and the encoder sorts them by channel and sums every fixed
/// predictor's residuals as they arrive, so `encode_block` only chooses the coding and writes it.
/// Nothing is allocated after `new`: the block (32 KB), the partition sums (2.5 KB) and the Rice
/// parameters (64 bytes) are reused.
pub struct Encoder {
    /// The block so far, one channel after the other, `BLOCK_FRAMES` clipped samples each.
    samples: Vec<i32>,
    /// Frames pushed into the block.
    filled: usize,
    /// Per channel: the last sample and its differences of order 1-3, which the next sample's
    /// residuals build on.
    previous: [[i32; MAX_FIXED_ORDER]; CHANNELS],
    /// Per channel and order: the folded residuals since the last `SUM_FRAMES` boundary.
    partial: [[u32; MAX_FIXED_ORDER + 1]; CHANNELS],
    /// Folded residuals summed per finest partition: a row of `PARTITIONS` for each channel and
    /// order (see `row`). The partition search merges a row in place.
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
        Self {
            samples: vec![0; CHANNELS * BLOCK_FRAMES],
            filled: 0,
            previous: [[0; MAX_FIXED_ORDER]; CHANNELS],
            partial: [[0; MAX_FIXED_ORDER + 1]; CHANNELS],
            sums: vec![0; CHANNELS * (MAX_FIXED_ORDER + 1) * PARTITIONS],
            parameters: vec![0; PARTITIONS],
            best_parameters: vec![0; PARTITIONS],
        }
    }

    /// Frames the block still needs before `encode_block`.
    pub fn frames_wanted(&self) -> usize {
        BLOCK_FRAMES - self.filled
    }

    /// Adds interleaved frames, left then right, to the block: sign-extended 24-bit samples, the
    /// stream ring's format. Samples beyond 24 bits clip at the rails, as in `wav::pack_s24le`.
    ///
    /// Panics if `interleaved` holds more than `frames_wanted()` frames.
    pub fn push(&mut self, interleaved: &[i32]) {
        let frames = interleaved.len() / CHANNELS;
        assert!(
            frames <= self.frames_wanted(),
            "a block is {BLOCK_FRAMES} frames"
        );
        for channel in 0..CHANNELS {
            // Local copies, so the loop keeps them in registers.
            let mut previous = self.previous[channel];
            let mut partial = self.partial[channel];
            let start = channel * BLOCK_FRAMES + self.filled;
            let column = &mut self.samples[start..start + frames];
            for (n, (sample, frame)) in
                (self.filled..).zip(column.iter_mut().zip(interleaved.chunks_exact(CHANNELS)))
            {
                // The fixed predictor of order k leaves the k-th difference of the samples (RFC
                // 9639, section 9.2.5), so each order's residual is the difference of the order
                // below's.
                let mut difference = clip(frame[channel]);
                *sample = difference;
                for (order, sum) in partial.iter_mut().enumerate() {
                    // The first `order` samples are the predictor's warm-up: they have no residual.
                    if n >= order {
                        *sum += fold(difference);
                    }
                    if order < MAX_FIXED_ORDER {
                        (previous[order], difference) = (difference, difference - previous[order]);
                    }
                }
                if (n + 1) % SUM_FRAMES == 0 {
                    let partition = n / PARTITION_FRAMES;
                    for (order, sum) in partial.iter_mut().enumerate() {
                        self.sums[row(channel, order) + partition] += u64::from(*sum);
                        *sum = 0;
                    }
                }
            }
            self.previous[channel] = previous;
            self.partial[channel] = partial;
        }
        self.filled += frames;
    }

    /// Replaces `out` with the FLAC frame for the block and starts the next block. `block_number`
    /// counts the blocks before this one. A frame keeps 31 bits of it, so it wraps after 2^31
    /// blocks, 5.8 years at 48 kHz.
    ///
    /// Panics unless the block has all its `BLOCK_FRAMES` frames.
    pub fn encode_block(&mut self, block_number: u32, out: &mut Vec<u8>) {
        assert_eq!(self.frames_wanted(), 0, "a block is {BLOCK_FRAMES} frames");
        out.clear();
        push_frame_header(block_number & FRAME_NUMBER_MASK, out);
        let header_crc = crc16(out);
        let mut bits = BitWriter::new(out, header_crc);
        for channel in 0..CHANNELS {
            self.write_subframe(channel, &mut bits);
        }
        let crc = bits.finish();
        out.extend_from_slice(&crc.to_be_bytes());
        // Each subframe stands alone, so the next block starts afresh.
        self.filled = 0;
        self.previous = [[0; MAX_FIXED_ORDER]; CHANNELS];
        self.sums.fill(0);
    }

    /// Writes an exact constant channel as one sample, otherwise a fixed-predictor subframe with
    /// partitioned Rice codes, or verbatim when that comes out no smaller than 24 bits a sample.
    /// The predictor is the order whose
    /// residuals add up smallest, as libFLAC picks among its fixed predictors; the partitioning and
    /// the Rice parameters are the ones with the smallest estimated size.
    fn write_subframe(&mut self, channel: usize, bits: &mut BitWriter) {
        // Order 1 skips the first sample: a zero sum means every clipped sample equals the
        // one before it. Reuse the sums from `push`, without scanning the block again.
        let first_difference = row(channel, 1);
        if self.sums[first_difference..first_difference + PARTITIONS]
            .iter()
            .all(|&sum| sum == 0)
        {
            bits.put(8, 0); // constant subframe, no wasted bits (RFC 9639, section 9.2.3)
            bits.put(
                BITS_PER_SAMPLE,
                self.samples[channel * BLOCK_FRAMES] as u32 & SAMPLE_MASK,
            );
            return;
        }
        let order = (0..=MAX_FIXED_ORDER)
            .min_by_key(|&order| {
                let row = row(channel, order);
                self.sums[row..row + PARTITIONS].iter().sum::<u64>()
            })
            .unwrap_or(0);
        let (partition_order, residual_bits) = self.best_partitioning(channel, order);
        let warm_up_bits = order as u64 * u64::from(BITS_PER_SAMPLE);
        let samples = &self.samples[channel * BLOCK_FRAMES..(channel + 1) * BLOCK_FRAMES];
        if SUBFRAME_HEADER_BITS + warm_up_bits + residual_bits < VERBATIM_SUBFRAME_BITS {
            let parameters = &self.best_parameters[..1 << partition_order];
            write_fixed(samples, order, parameters, bits);
        } else {
            write_verbatim(samples, bits);
        }
    }

    /// The partition order whose Rice codes come out smallest for `channel`'s residual of `order`,
    /// and their estimated bits, from its row of `self.sums`, which it merges in place. The chosen
    /// partitions' Rice parameters end up in `self.best_parameters`.
    fn best_partitioning(&mut self, channel: usize, order: usize) -> (u32, u64) {
        let row = row(channel, order);
        let mut best = (MAX_PARTITION_ORDER, u64::MAX);
        for partition_order in (0..=MAX_PARTITION_ORDER).rev() {
            let partitions = 1 << partition_order;
            let len = (BLOCK_FRAMES >> partition_order) as u32;
            let mut bits = RESIDUAL_HEADER_BITS;
            for j in 0..partitions {
                // The first partition starts after the warm-up samples.
                let residuals = if j == 0 { len - order as u32 } else { len };
                let sum = self.sums[row + j];
                let k = rice_parameter(residuals, sum);
                self.parameters[j] = k as u8;
                bits += PARAMETER_BITS_ESTIMATE + rice_bits(residuals, sum, k);
            }
            if bits < best.1 {
                best = (partition_order, bits);
                self.best_parameters[..partitions].copy_from_slice(&self.parameters[..partitions]);
            }
            // Pairs of partitions make the next order's partitions.
            for j in 0..partitions / 2 {
                self.sums[row + j] = self.sums[row + 2 * j] + self.sums[row + 2 * j + 1];
            }
        }
        best
    }
}

/// Where the partition sums of `channel`'s residual of `order` start in `Encoder::sums`.
fn row(channel: usize, order: usize) -> usize {
    (channel * (MAX_FIXED_ORDER + 1) + order) * PARTITIONS
}

/// Writes a fixed-predictor subframe (RFC 9639, sections 9.2.5 and 9.2.7) for one channel's
/// `samples`, with a Rice parameter for each of `parameters.len()` partitions.
fn write_fixed(samples: &[i32], order: usize, parameters: &[u8], bits: &mut BitWriter) {
    match order {
        0 => write_fixed_order::<0>(samples, parameters, bits),
        1 => write_fixed_order::<1>(samples, parameters, bits),
        2 => write_fixed_order::<2>(samples, parameters, bits),
        3 => write_fixed_order::<3>(samples, parameters, bits),
        _ => write_fixed_order::<4>(samples, parameters, bits),
    }
}

/// `write_fixed` for one predictor order, so that its arithmetic is fixed when this is compiled.
fn write_fixed_order<const ORDER: usize>(samples: &[i32], parameters: &[u8], bits: &mut BitWriter) {
    bits.put(8, (FIXED_TYPE | ORDER as u32) << 1);
    for &sample in &samples[..ORDER] {
        bits.put(BITS_PER_SAMPLE, sample as u32 & SAMPLE_MASK); // the warm-up samples, unencoded
    }
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
    // There are 2^(partition order) partitions.
    bits.put(4, parameters.len().trailing_zeros());
    let len = BLOCK_FRAMES / parameters.len();
    for (j, &k) in parameters.iter().enumerate() {
        bits.put(parameter_bits, u32::from(k));
        // Each residual needs the `ORDER` samples before it; the first partition skips the warm-up.
        let first = (j * len).max(ORDER);
        for window in samples[first - ORDER..(j + 1) * len].windows(ORDER + 1) {
            bits.put_rice(u32::from(k), fold(fixed_residual::<ORDER>(window)));
        }
    }
}

/// The residual of the fixed predictor of `ORDER` for the last sample of `window`, which holds the
/// `ORDER` samples before it too (RFC 9639, section 9.2.5). For 24-bit samples it stays within 28 bits.
fn fixed_residual<const ORDER: usize>(window: &[i32]) -> i32 {
    match ORDER {
        0 => window[0],
        1 => window[1] - window[0],
        2 => window[2] - 2 * window[1] + window[0],
        3 => window[3] - 3 * window[2] + 3 * window[1] - window[0],
        _ => window[4] - 4 * window[3] + 6 * window[2] - 4 * window[1] + window[0],
    }
}

/// Writes one channel's `samples` as a verbatim subframe: every sample unencoded (RFC 9639, section
/// 9.2.4).
fn write_verbatim(samples: &[i32], bits: &mut BitWriter) {
    bits.put(8, VERBATIM_TYPE << 1);
    for &sample in samples {
        bits.put(BITS_PER_SAMPLE, sample as u32 & SAMPLE_MASK);
    }
}

/// A sample clipped to the 24-bit rails.
fn clip(sample: i32) -> i32 {
    sample.clamp(SAMPLE_MIN, SAMPLE_MAX)
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

/// A residual as Rice codes store it: 2r for r at or above 0, and -2r - 1 below (RFC 9639, section
/// 9.2.7.2).
fn fold(r: i32) -> u32 {
    ((r << 1) ^ (r >> 31)) as u32
}

/// The Rice parameter that codes `n` folded residuals summing to `sum` in the fewest bits by
/// `rice_bits`'s estimate. Going from k to k + 1 saves (sum >> k) - (sum >> (k + 1)), which is
/// (sum >> k) / 2 rounded up, and costs n, so the best k is the first with sum >> k at most 2n. With
/// d the difference of the bit lengths of sum and 2n, that's d or d + 1.
fn rice_parameter(n: u32, sum: u64) -> u32 {
    let twice_n = 2 * u64::from(n);
    let d = (u64::BITS - sum.leading_zeros()).saturating_sub(u64::BITS - twice_n.leading_zeros());
    let k = if sum >> d > twice_n { d + 1 } else { d };
    k.min(MAX_RICE5_PARAMETER)
}

/// The bits `n` folded residuals summing to `sum` take as Rice codes with parameter `k`, by the
/// estimate n * (k + 1) + (sum >> k): a stop bit and k low bits per residual, and the unary high
/// parts. It never falls short, because the high parts, each residual >> k, add up to at most sum >> k.
fn rice_bits(n: u32, sum: u64, k: u32) -> u64 {
    u64::from(n * (k + 1)) + (sum >> k)
}

/// Writes bits most significant first, as FLAC stores everything, and appends them to `out` a
/// 32-bit word at a time. It keeps the CRC-16 of what it appends, so a frame's bytes needn't be read
/// back for the frame's check.
struct BitWriter<'a> {
    out: &'a mut Vec<u8>,
    /// Bits not yet appended, in the low `pending` bits.
    word: u32,
    /// Below 32 between calls.
    pending: u32,
    /// The CRC-16 of the bytes before the writer started, then of everything it has appended too.
    crc: u16,
}

impl<'a> BitWriter<'a> {
    /// A writer that appends to `out`, whose bytes so far have the CRC-16 `crc`.
    fn new(out: &'a mut Vec<u8>, crc: u16) -> Self {
        Self {
            out,
            word: 0,
            pending: 0,
            crc,
        }
    }

    /// Writes `value` in `n` bits, `n` at most 32; `value` must fit in them.
    #[inline(always)]
    fn put(&mut self, n: u32, value: u32) {
        debug_assert!(n <= 32 && (n == 32 || value >> n == 0));
        if n < 32 - self.pending {
            self.word = (self.word << n) | value;
            self.pending += n;
        } else {
            self.put_filling_word(n, value);
        }
    }

    /// `put` when `value` fills the word: fills it with the top bits of `value`, appends it, and
    /// keeps the other bits. Out of line, so `put` stays small where it's inlined.
    #[inline(never)]
    fn put_filling_word(&mut self, n: u32, value: u32) {
        let room = 32 - self.pending;
        let left = n - room;
        let full = if room == 32 {
            value
        } else {
            (self.word << room) | (value >> left)
        };
        self.append(&full.to_be_bytes());
        self.word = value & ((1 << left) - 1);
        self.pending = left;
    }

    /// Writes a folded residual as a Rice code word with parameter `k`: its high part in unary (that
    /// many 0s, then a 1), then its low `k` bits.
    #[inline(always)]
    fn put_rice(&mut self, k: u32, folded: u32) {
        let high = folded >> k;
        let low = folded & ((1 << k) - 1);
        if high + 1 + k <= 32 {
            // The high part's 0s come from the field's width.
            self.put(high + 1 + k, (1 << k) | low);
        } else {
            let mut zeros = high;
            while zeros >= 32 {
                self.put(32, 0);
                zeros -= 32;
            }
            self.put(zeros + 1, 1);
            self.put(k, low);
        }
    }

    fn append(&mut self, bytes: &[u8]) {
        self.crc = bytes
            .iter()
            .fold(self.crc, |crc, &byte| crc16_step(crc, byte));
        self.out.extend_from_slice(bytes);
    }

    /// Pads with 0s to a whole byte, appends what's left, and returns the CRC-16 of all of `out`.
    fn finish(mut self) -> u16 {
        if self.pending > 0 {
            let bytes = (self.word << (32 - self.pending)).to_be_bytes();
            self.append(&bytes[..self.pending.div_ceil(8) as usize]);
        }
        self.crc
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
    bytes.iter().fold(0, |crc, &byte| crc16_step(crc, byte))
}

/// The CRC-16 after one more byte.
fn crc16_step(crc: u16, byte: u8) -> u16 {
    (crc << 8) ^ CRC16_TABLE[usize::from((crc >> 8) as u8 ^ byte)]
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

    /// Chunk sizes, in frames, that cut a block in uneven places, partitions included. The stream
    /// ring hands a listener up to 240 frames at a time, and fewer when it has caught up.
    const CHUNKS: [usize; 6] = [240, 1, 17, 240, 100, 33];

    /// Pushes `block` into `encoder` in chunks of `sizes` frames, repeating.
    fn push_in_chunks(encoder: &mut Encoder, block: &[i32], sizes: &[usize]) {
        let mut rest = block;
        for &frames in sizes.iter().cycle() {
            if rest.is_empty() {
                break;
            }
            let (chunk, tail) = rest.split_at(CHANNELS * frames.min(rest.len() / CHANNELS));
            encoder.push(chunk);
            rest = tail;
        }
    }

    /// Pushes `block` into `encoder` in chunks of `sizes` frames, then encodes it as block `number`
    /// into `out`.
    fn push_and_encode(
        encoder: &mut Encoder,
        block: &[i32],
        sizes: &[usize],
        number: u32,
        out: &mut Vec<u8>,
    ) {
        push_in_chunks(encoder, block, sizes);
        assert_eq!(encoder.frames_wanted(), 0, "the block is complete");
        encoder.encode_block(number, out);
    }

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
            push_and_encode(&mut encoder, block, &CHUNKS, n as u32, &mut frame);
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
                push_and_encode(&mut encoder, block, &CHUNKS, 0, &mut frame);
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
    fn constant_channels_use_four_byte_subframes() {
        for (left, right) in [(0, 0), (123_456, -654_321), (i32::MAX, i32::MIN)] {
            let frames: Vec<i32> = [left, right]
                .into_iter()
                .cycle()
                .take(CHANNELS * BLOCK_FRAMES)
                .collect();
            let stream = encode_stream(&frames);
            assert_eq!(
                stream.len() - STREAM_HEADER_LEN,
                16,
                "constant stereo frame"
            );
            let frame = &stream[STREAM_HEADER_LEN..];
            assert_eq!(frame[6], 0, "left constant subframe header");
            assert_eq!(frame[10], 0, "right constant subframe header");
            let expected: Vec<i32> = frames.into_iter().map(clip).collect();
            assert_same_samples(&decode_with_claxon(&stream).1, &expected);
        }
    }

    #[test]
    fn constant_channels_change_independently_between_blocks() {
        let noise = full_scale_noise_stereo(BLOCK_FRAMES);
        let mut frames = Vec::new();
        for block in 0..4 {
            for pair in noise.chunks_exact(CHANNELS) {
                frames.extend_from_slice(&match block {
                    0 => [321, pair[1]],
                    1 => [pair[0], -123],
                    2 => [0, 0],
                    _ => [pair[0], pair[1]],
                });
            }
        }
        let mixed = 6 + 4 + 1 + 3 * BLOCK_FRAMES + 2;
        assert_eq!(
            frame_sizes(&frames),
            [mixed, mixed, 16, 6 + 2 * (1 + 3 * BLOCK_FRAMES) + 2]
        );
        assert_same_samples(&decode_with_claxon(&encode_stream(&frames)).1, &frames);
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
        push_and_encode(
            &mut Encoder::new(),
            &full_scale_noise_stereo(BLOCK_FRAMES),
            &CHUNKS,
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
            push_and_encode(&mut encoder, &block, &CHUNKS, n, &mut frame);
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
        // Silence stores only one 24-bit sample per channel.
        let silence = frame_sizes(&vec![0; 2 * CHANNELS * BLOCK_FRAMES]);
        assert!(
            silence.iter().all(|&size| size < PCM_BLOCK_BYTES / 16),
            "silent frames {silence:?}"
        );
    }

    #[test]
    fn the_frame_does_not_depend_on_how_the_block_was_pushed() {
        let block = chord_and_chirp_stereo_48k(BLOCK_FRAMES);
        let mut encoder = Encoder::new();
        let mut whole = vec![];
        push_and_encode(&mut encoder, &block, &[BLOCK_FRAMES], 0, &mut whole);
        for sizes in [&CHUNKS[..], &[1], &[16], &[15, 2]] {
            let mut frame = vec![];
            push_and_encode(&mut encoder, &block, sizes, 0, &mut frame);
            assert!(frame == whole, "pushed in chunks of {sizes:?}");
        }
    }

    #[test]
    fn partition_sums_match_the_fixed_predictors_residuals() {
        // Noise over a tone, pushed in uneven chunks.
        let block: Vec<i32> = chord_and_chirp_stereo_48k(BLOCK_FRAMES)
            .iter()
            .zip(full_scale_noise_stereo(BLOCK_FRAMES))
            .map(|(&tone, noise)| tone / 2 + noise / 4)
            .collect();
        let mut encoder = Encoder::new();
        push_in_chunks(&mut encoder, &block, &CHUNKS);
        // The fixed predictors as RFC 9639, section 9.2.5 writes them.
        let residual = |s: &[i32], order: usize, n: usize| -> i32 {
            match order {
                0 => s[n],
                1 => s[n] - s[n - 1],
                2 => s[n] - 2 * s[n - 1] + s[n - 2],
                3 => s[n] - 3 * s[n - 1] + 3 * s[n - 2] - s[n - 3],
                _ => s[n] - 4 * s[n - 1] + 6 * s[n - 2] - 4 * s[n - 3] + s[n - 4],
            }
        };
        let len = BLOCK_FRAMES / PARTITIONS;
        for channel in 0..CHANNELS {
            let samples: Vec<i32> = block
                .iter()
                .skip(channel)
                .step_by(CHANNELS)
                .copied()
                .collect();
            for order in 0..=MAX_FIXED_ORDER {
                for j in 0..PARTITIONS {
                    let expected: u64 = ((j * len).max(order)..(j + 1) * len)
                        .map(|n| u64::from(fold(residual(&samples, order, n))))
                        .sum();
                    assert_eq!(
                        encoder.sums[row(channel, order) + j],
                        expected,
                        "channel {channel}, order {order}, partition {j}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_rice_parameter_is_the_cheapest_by_the_estimate() {
        for n in [12, 13, 16, 256, 4092, 4096] {
            for mean in [0, 1, 2, 3, 7, 100, 1_000, 65_535, 1 << 20, (1 << 28) - 15] {
                for sum in [n as u64 * mean, n as u64 * mean + n as u64 / 2] {
                    let cheapest = (0..=MAX_RICE5_PARAMETER)
                        .map(|k| rice_bits(n, sum, k))
                        .min()
                        .unwrap();
                    let k = rice_parameter(n, sum);
                    assert_eq!(rice_bits(n, sum, k), cheapest, "n {n}, sum {sum}: k {k}");
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "4096 frames")]
    fn a_block_takes_no_more_than_4096_frames() {
        let mut encoder = Encoder::new();
        encoder.push(&vec![0; CHANNELS * (BLOCK_FRAMES - 1)]);
        encoder.push(&[0; 2 * CHANNELS]);
    }

    #[test]
    #[should_panic(expected = "4096 frames")]
    fn a_block_is_encoded_only_when_complete() {
        let mut encoder = Encoder::new();
        encoder.push(&[0; 2 * 100]);
        encoder.encode_block(0, &mut vec![]);
    }
}
