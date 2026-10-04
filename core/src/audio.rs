// ABOUTME: Audio helpers for the capture path: I2S slot unpacking, a sample ring, a level meter, loudness, a tone check.
// ABOUTME: Pure code, so the Mac tests cover what the firmware runs.

pub const SILENCE_DBFS: f32 = -120.0;

/// Fixed-capacity ring of mono samples that keeps the newest audio.
#[derive(Debug)]
pub struct SampleRing {
    buf: Vec<i16>,
    write: usize,
    filled: usize,
}

impl SampleRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0; capacity],
            write: 0,
            filled: 0,
        }
    }
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }
    pub fn len(&self) -> usize {
        self.filled
    }
    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }
    pub fn push(&mut self, samples: &[i16]) {
        let cap = self.buf.len();
        for &s in samples {
            self.buf[self.write] = s;
            self.write = (self.write + 1) % cap;
        }
        self.filled = (self.filled + samples.len()).min(cap);
    }

    pub fn copy_latest(&self, n: usize, out: &mut Vec<i16>) {
        out.clear();
        let n = n.min(self.filled);
        let cap = self.buf.len();
        let start = (self.write + cap - n) % cap;
        out.extend((0..n).map(|i| self.buf[(start + i) % cap]));
    }
}

/// Loudness of a block in dBFS after removing its DC offset. Silence and empty input give SILENCE_DBFS.
pub fn loudness_dbfs(samples: &[i16]) -> f32 {
    Sums::of(samples).dbfs()
}

/// The count, sum and sum of squares of a run of samples: all that loudness needs, and two runs'
/// sums add up to the sums of both.
#[derive(Clone, Copy, Debug, Default)]
struct Sums {
    count: u64,
    sum: i64,
    sum_sq: i64,
}

impl Sums {
    fn of(samples: &[i16]) -> Self {
        // Integer sums keep this cheap on the ESP32, which has no double-precision FPU.
        let (sum, sum_sq) = samples.iter().fold((0i64, 0i64), |(s, q), &x| {
            let x = i64::from(x);
            (s + x, q + x * x)
        });
        Self {
            count: samples.len() as u64,
            sum,
            sum_sq,
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            count: self.count + other.count,
            sum: self.sum + other.sum,
            sum_sq: self.sum_sq + other.sum_sq,
        }
    }

    fn dbfs(self) -> f32 {
        if self.count == 0 {
            return SILENCE_DBFS;
        }
        let n = self.count as f64;
        let mean = self.sum as f64 / n;
        let variance = self.sum_sq as f64 / n - mean * mean;
        // Anything under one LSB of RMS is rounding error from the DC removal, not sound.
        if variance < 1.0 {
            return SILENCE_DBFS;
        }
        let rms = variance.sqrt() / 32_768.0;
        ((20.0 * rms.log10()) as f32).max(SILENCE_DBFS)
    }
}

/// Replaces `out` with the samples in `bytes`: 32-bit I2S slots, little-endian, each carrying a
/// 24-bit sample in its top 24 bits. The result holds each sample sign-extended in an `i32`, the
/// format `Decimator` and `StreamRing` take. The low byte is whatever the line carried after the
/// sample, so it is dropped, and so are bytes short of a whole slot at the end.
pub fn samples_from_i2s_slots(bytes: &[u8], out: &mut Vec<i32>) {
    out.clear();
    out.extend(
        bytes
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) >> 8), // `>>` on i32 keeps the sign
    );
}

/// The loudness of the newest `window` samples of a feed, read each time another `interval` of
/// samples has arrived. It keeps sums per interval, not samples: a reading adds up a few sums, and
/// the audio thread meters without touching the 12 s ring or PSRAM.
pub struct LevelMeter {
    /// Sums of the newest whole intervals, `window / interval` of them; the oldest is overwritten next.
    intervals: Vec<Sums>,
    next: usize,
    /// Sums of the interval still filling.
    filling: Sums,
    interval: usize,
}

impl LevelMeter {
    /// `window` must be a whole number of intervals.
    pub fn new(window: usize, interval: usize) -> Self {
        assert!(
            interval > 0 && window >= interval && window.is_multiple_of(interval),
            "a level meter's window must be a whole number of intervals of at least one sample"
        );
        Self {
            intervals: vec![Sums::default(); window / interval],
            next: 0,
            filling: Sums::default(),
            interval,
        }
    }

    /// Adds `samples`. Each interval that fills makes a reading: the loudness in dBFS of the newest
    /// `window` samples up to the end of that interval (of all samples so far, before a whole window
    /// has arrived). Returns the last reading this push made, if any.
    pub fn push(&mut self, mut samples: &[i16]) -> Option<f32> {
        let mut reading = None;
        while !samples.is_empty() {
            let room = self.interval - self.filling.count as usize;
            let (now, later) = samples.split_at(room.min(samples.len()));
            self.filling = self.filling.plus(Sums::of(now));
            samples = later;
            if self.filling.count as usize == self.interval {
                self.intervals[self.next] = std::mem::take(&mut self.filling);
                self.next = (self.next + 1) % self.intervals.len();
                let window = self
                    .intervals
                    .iter()
                    .fold(Sums::default(), |a, &b| a.plus(b));
                reading = Some(window.dbfs());
            }
        }
        reading
    }
}

/// Averages interleaved stereo frames (L, R, L, R, ...) into mono. A trailing odd sample is dropped.
pub fn stereo_to_mono(interleaved: &[i16], out: &mut Vec<i16>) {
    out.clear();
    out.extend(
        interleaved
            .chunks_exact(2)
            .map(|frame| ((i32::from(frame[0]) + i32::from(frame[1])) / 2) as i16),
    );
}

/// Frequency of a clean tone estimated from zero crossings. Bring-up plays a 1 kHz tone into
/// LINE IN; reading ~1000 proves the I2S format and sample rate are right.
pub fn zero_crossing_hz(samples: &[i16], sample_rate_hz: u32) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    let mean = samples.iter().map(|&s| i64::from(s)).sum::<i64>() / samples.len() as i64;
    let crossings = samples
        .windows(2)
        .filter(|w| (i64::from(w[0]) < mean) != (i64::from(w[1]) < mean))
        .count();
    crossings as f32 * sample_rate_hz as f32 / (2.0 * samples.len() as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq_hz: f32, amplitude: f32, seconds: f32, dc: i16) -> Vec<i16> {
        let n = (16_000.0 * seconds) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / 16_000.0;
                let v = amplitude * 32_767.0 * (2.0 * std::f32::consts::PI * freq_hz * t).sin();
                (v as i32 + dc as i32) as i16
            })
            .collect()
    }

    #[test]
    fn ring_returns_newest_samples_oldest_first() {
        let mut ring = SampleRing::new(5);
        let mut out = vec![];
        ring.push(&[1, 2, 3]);
        ring.copy_latest(10, &mut out);
        assert_eq!(out, vec![1, 2, 3]);
        ring.push(&[4, 5, 6, 7]);
        assert_eq!(ring.len(), 5);
        ring.copy_latest(5, &mut out);
        assert_eq!(out, vec![3, 4, 5, 6, 7]);
        ring.copy_latest(2, &mut out);
        assert_eq!(out, vec![6, 7]);
    }

    #[test]
    fn silence_and_empty_input_read_as_silence() {
        assert_eq!(loudness_dbfs(&[]), SILENCE_DBFS);
        assert_eq!(loudness_dbfs(&[0; 16_000]), SILENCE_DBFS);
    }

    #[test]
    fn half_scale_sine_reads_minus_nine_dbfs() {
        // RMS of a sine is amplitude / sqrt(2): 0.5 -> -9.03 dBFS.
        let level = loudness_dbfs(&sine(1_000.0, 0.5, 1.0, 0));
        assert!((level - -9.03).abs() < 0.05, "got {level}");
    }

    #[test]
    fn dc_offset_alone_reads_as_silence() {
        // A plugged-in cable with nothing playing can sit at a DC offset; it must not look loud.
        assert_eq!(loudness_dbfs(&[10_000; 16_000]), SILENCE_DBFS);
        let level = loudness_dbfs(&sine(1_000.0, 0.1, 1.0, 10_000));
        assert!((level - -23.01).abs() < 0.05, "got {level}");
    }

    #[test]
    fn stereo_frames_average_to_mono() {
        let mut out = vec![];
        stereo_to_mono(&[100, 300, -100, -300, 7], &mut out);
        assert_eq!(out, vec![200, -200]);
    }

    #[test]
    fn zero_crossings_find_a_one_khz_tone() {
        let hz = zero_crossing_hz(&sine(1_000.0, 0.5, 1.0, 500), 16_000);
        assert!((hz - 1_000.0).abs() < 2.0, "got {hz}");
    }

    /// 32-bit I2S slot words as the DMA buffer holds them: little-endian.
    fn slot_bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn slots_become_sign_extended_24_bit_samples() {
        let mut out = vec![99];
        samples_from_i2s_slots(
            &slot_bytes(&[0x7FFF_FF00, 0x8000_0000, 0xFFFF_FF00, 0x0000_0100, 0]),
            &mut out,
        );
        assert_eq!(out, vec![8_388_607, -8_388_608, -1, 1, 0]);
    }

    #[test]
    fn the_low_byte_of_a_slot_is_not_part_of_the_sample() {
        // The codec's 24 bits end at bit 8; whatever the line carries after them must not leak in.
        let mut out = vec![];
        samples_from_i2s_slots(
            &slot_bytes(&[0x0000_01FF, 0xFFFF_FFFF, 0x8000_00FF]),
            &mut out,
        );
        assert_eq!(out, vec![1, -1, -8_388_608]);
    }

    #[test]
    fn a_partial_slot_at_the_end_is_dropped() {
        let mut bytes = slot_bytes(&[0x0000_0200]);
        bytes.extend([0xAA, 0xBB, 0xCC]);
        let mut out = vec![];
        samples_from_i2s_slots(&bytes, &mut out);
        assert_eq!(out, vec![2]);
    }

    #[test]
    fn the_meter_reads_once_every_interval() {
        let mut meter = LevelMeter::new(16_000, 4_000);
        let tone = sine(1_000.0, 0.5, 2.0, 0);
        let mut reading_at = vec![];
        let mut total = 0;
        for chunk in tone.chunks(80) {
            total += chunk.len();
            if meter.push(chunk).is_some() {
                reading_at.push(total);
            }
        }
        assert_eq!(reading_at, (1..=8).map(|k| k * 4_000).collect::<Vec<_>>());
    }

    #[test]
    fn a_push_longer_than_the_interval_reads_once_and_keeps_the_cadence() {
        let mut meter = LevelMeter::new(16_000, 4_000);
        let tone = sine(1_000.0, 0.5, 1.0, 0);
        assert!(meter.push(&tone[..9_000]).is_some());
        assert!(meter.push(&tone[9_000..11_999]).is_none());
        assert!(meter.push(&tone[11_999..12_000]).is_some());
    }

    #[test]
    fn a_reading_ends_where_the_last_interval_ended() {
        // One push of 8,000 samples of tone and 1,000 of silence fills two intervals; the reading
        // covers those two and none of the silence after them (which would make it 0.5 dB quieter).
        let mut meter = LevelMeter::new(16_000, 4_000);
        let tone = sine(1_000.0, 0.5, 0.5, 0);
        let pushed = [&tone[..], &[0; 1_000]].concat();
        assert_eq!(meter.push(&pushed), Some(loudness_dbfs(&tone)));
    }

    #[test]
    fn early_readings_cover_only_what_has_arrived() {
        // Before a whole window has arrived, the reading is the level of what has: an empty window
        // padded with zeros would read a quarter second of this tone 6 dB too quiet.
        let mut meter = LevelMeter::new(16_000, 4_000);
        let tone = sine(1_000.0, 0.5, 0.25, 0);
        let level = meter.push(&tone).expect("a reading after 4,000 samples");
        assert_eq!(level, loudness_dbfs(&tone));
        assert!((level - -9.03).abs() < 0.05, "got {level}");
    }

    #[test]
    fn a_reading_covers_only_the_newest_window() {
        let mut meter = LevelMeter::new(16_000, 4_000);
        let tone = sine(1_000.0, 0.5, 1.0, 0);
        let silence = vec![0i16; 16_000];
        for chunk in tone.chunks(80) {
            meter.push(chunk);
        }
        let mut readings = vec![];
        for chunk in silence.chunks(80) {
            if let Some(level) = meter.push(chunk) {
                readings.push(level);
            }
        }
        // A quarter second into the silence, the window holds the last 0.75 s of the tone: 1.25 dB down.
        let expected = loudness_dbfs(&[&tone[4_000..], &silence[..4_000]].concat());
        assert_eq!(readings[0], expected);
        assert!((readings[0] - -10.28).abs() < 0.05, "got {}", readings[0]);
        // A whole second of silence leaves nothing of the tone in the window.
        assert_eq!(readings.len(), 4);
        assert_eq!(readings[3], SILENCE_DBFS);
    }
}
