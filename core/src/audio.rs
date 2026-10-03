// ABOUTME: Audio helpers for the capture path: a fixed-size sample ring, loudness, and a tone check.
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
    if samples.is_empty() {
        return SILENCE_DBFS;
    }
    // Integer sums keep this cheap on the ESP32, which has no double-precision FPU.
    let (sum, sum_sq) = samples.iter().fold((0i64, 0i64), |(s, q), &x| {
        let x = i64::from(x);
        (s + x, q + x * x)
    });
    let n = samples.len() as f64;
    let mean = sum as f64 / n;
    let variance = sum_sq as f64 / n - mean * mean;
    // Anything under one LSB of RMS is rounding error from the DC removal, not sound.
    if variance < 1.0 {
        return SILENCE_DBFS;
    }
    let rms = variance.sqrt() / 32_768.0;
    ((20.0 * rms.log10()) as f32).max(SILENCE_DBFS)
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
}
