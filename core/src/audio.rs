// ABOUTME: Audio helpers for the capture path: I2S slot unpacking, a sample ring, a level meter, loudness, a tone check.
// ABOUTME: Pure code, so the Mac tests cover what the firmware runs.

use std::sync::{Arc, Mutex, TryLockError};

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
    pub fn clear(&mut self) {
        self.write = 0;
        self.filled = 0;
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

/// Fingerprint samples waiting for a snapshot to release the recognition ring.
/// Capture never waits for this lock, so a snapshot cannot stall the I2S reader.
pub struct RecognitionBuffer {
    ring: Arc<Mutex<SampleRing>>,
    waiting: Vec<i16>,
    waiting_limit: usize,
    reset_pending: bool,
}

impl RecognitionBuffer {
    /// Discards pre-gap samples; clearing the shared ring waits without blocking capture.
    pub fn reset(&mut self) {
        self.waiting.clear();
        self.reset_pending = true;
        self.push(&[]);
    }
    /// Disabled recognition has neither a sample ring nor a waiting buffer.
    pub fn new(
        enabled: bool,
        window: usize,
        waiting_limit: usize,
        chunk_capacity: usize,
    ) -> Option<Self> {
        if !enabled {
            return None;
        }
        Some(Self {
            ring: Arc::new(Mutex::new(SampleRing::new(window))),
            waiting: Vec::with_capacity(waiting_limit + chunk_capacity),
            waiting_limit,
            reset_pending: false,
        })
    }

    pub fn shared_ring(&self) -> Arc<Mutex<SampleRing>> {
        self.ring.clone()
    }

    pub fn push(&mut self, samples: &[i16]) {
        self.waiting.extend_from_slice(samples);
        match self.ring.try_lock() {
            Ok(mut ring) => {
                if self.reset_pending {
                    ring.clear();
                    self.reset_pending = false;
                }
                ring.push(&self.waiting);
                self.waiting.clear();
            }
            // The recognizer is copying its 12 s window out (about 50 ms). Waiting for the lock
            // would overrun the DMA queue, so the samples wait here for a later chunk instead.
            Err(TryLockError::WouldBlock) => {
                let excess = self.waiting.len().saturating_sub(self.waiting_limit);
                if excess > 0 {
                    self.reset_pending = true;
                }
                self.waiting.drain(..excess);
            }
            Err(TryLockError::Poisoned(_)) => panic!("audio ring lock poisoned"),
        }
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
    /// Drops completed and partial intervals after a capture gap, retaining storage.
    pub fn reset(&mut self) {
        self.intervals.fill(Sums::default());
        self.next = 0;
        self.filling = Sums::default();
    }
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

/// AC loudness and raw sample diagnostics for one software channel.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct ChannelReading {
    /// DC-removed RMS in dBFS, floored at SILENCE_DBFS.
    pub rms_dbfs: f32,
    /// Raw absolute sample peak divided by 8,388,608; negative rail reaches 1.
    pub peak_normalized: f32,
    /// Samples equal to either signed 24-bit rail in this completed window.
    pub digital_rail_samples: u32,
    /// Signed mean sample divided by 8,388,608, including fractional sample means.
    pub dc_offset_normalized: f32,
}

/// A completed stereo window; combined loudness averages energy before conversion to dBFS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StereoReading {
    pub level_dbfs: f32,
    /// Actual frames per channel, excluding the filling interval and any half frame.
    pub window_frames: u32,
    pub channels: [ChannelReading; 2],
}

// Stereo diagnostics stay separate so mono metering keeps its existing storage size.
#[derive(Clone, Copy, Debug, Default)]
struct StereoSums {
    sums: Sums,
    peak: u32,
    digital_rail_samples: u32,
}

impl StereoSums {
    fn push(&mut self, sample: i32) {
        let x = i64::from(sample);
        self.sums.count += 1;
        self.sums.sum += x;
        self.sums.sum_sq += x * x;
        self.peak = self.peak.max(sample.unsigned_abs());
        self.digital_rail_samples += u32::from(sample == -8_388_608 || sample == 8_388_607);
    }

    fn plus(self, other: Self) -> Self {
        Self {
            sums: self.sums.plus(other.sums),
            peak: self.peak.max(other.peak),
            digital_rail_samples: self.digital_rail_samples + other.digital_rail_samples,
        }
    }

    fn variance(self) -> f64 {
        let n = self.sums.count as f64;
        let mean = self.sums.sum as f64 / n;
        (self.sums.sum_sq as f64 / n - mean * mean).max(0.0)
    }

    fn reading(self, variance: f64) -> ChannelReading {
        ChannelReading {
            rms_dbfs: stereo_energy_dbfs(variance),
            peak_normalized: (self.peak as f64 / 8_388_608.0) as f32,
            digital_rail_samples: self.digital_rail_samples,
            dc_offset_normalized: (self.sums.sum as f64 / self.sums.count as f64 / 8_388_608.0)
                as f32,
        }
    }
}

fn stereo_energy_dbfs(energy: f64) -> f32 {
    // Preserve the sub-one-sample-unit variance silence rule independently per channel.
    if energy < 1.0 {
        SILENCE_DBFS
    } else {
        ((10.0 * (energy / (8_388_608.0 * 8_388_608.0)).log10()) as f32).max(SILENCE_DBFS)
    }
}

/// One second of stereo energy, read every 250 ms, with DC removed independently per channel.
/// Four interval buckets keep capture allocation-free; samples never leave the caller's buffer.
pub struct StereoLevelMeter {
    intervals: [[StereoSums; 2]; 4],
    filling: [StereoSums; 2],
    next: usize,
    channel: usize,
    interval: usize,
}

impl StereoLevelMeter {
    /// `sample_rate` is stereo frames per second and must be divisible by four.
    /// At most 131,071 frames fit signed 64-bit square sums for signed 24-bit input.
    pub fn new(sample_rate: usize) -> Self {
        assert!(
            sample_rate > 0 && sample_rate <= 131_071 && sample_rate.is_multiple_of(4),
            "stereo meter rate must be divisible by four and fit 24-bit square sums"
        );
        Self {
            intervals: [[StereoSums::default(); 2]; 4],
            filling: [StereoSums::default(); 2],
            next: 0,
            channel: 0,
            interval: sample_rate / 4,
        }
    }

    /// Drops the window, partial interval and any half frame after a capture gap.
    pub fn reset(&mut self) {
        self.intervals.fill([StereoSums::default(); 2]);
        self.filling = [StereoSums::default(); 2];
        self.next = 0;
        self.channel = 0;
    }

    /// Takes sign-extended 24-bit samples in L, R order, retaining half frames across calls.
    /// Returns the last completed interval's reading; startup uses only frames received so far.
    /// Energy is averaged across channels, so a lone active channel reads 3 dB below two alike.
    pub fn push(&mut self, interleaved: &[i32]) -> Option<StereoReading> {
        let mut reading = None;
        for &sample in interleaved {
            debug_assert!((-8_388_608..=8_388_607).contains(&sample));
            self.filling[self.channel].push(sample);
            self.channel ^= 1;
            if self.channel == 0 && self.filling[1].sums.count as usize == self.interval {
                self.intervals[self.next] = std::mem::take(&mut self.filling);
                self.next = (self.next + 1) % self.intervals.len();
                let mut window = [StereoSums::default(); 2];
                for bucket in &self.intervals {
                    for channel in 0..2 {
                        window[channel] = window[channel].plus(bucket[channel]);
                    }
                }
                let variance = window.map(StereoSums::variance);
                reading = Some(StereoReading {
                    level_dbfs: stereo_energy_dbfs((variance[0] + variance[1]) / 2.0),
                    window_frames: window[0].sums.count as u32,
                    channels: [
                        window[0].reading(variance[0]),
                        window[1].reading(variance[1]),
                    ],
                });
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

    #[test]
    fn recognition_reset_clears_available_ring_immediately() {
        let mut recognition = RecognitionBuffer::new(true, 16, 8, 4).unwrap();
        recognition.push(&[1, 2]);
        let ring = recognition.shared_ring();
        recognition.reset();
        assert!(ring.lock().unwrap().is_empty());
    }

    #[test]
    fn recognition_reset_retry_clears_released_ring_without_new_feed() {
        let mut recognition = RecognitionBuffer::new(true, 16, 8, 4).unwrap();
        recognition.push(&[1, 2]);
        let ring = recognition.shared_ring();
        let held = ring.lock().unwrap();
        recognition.reset();
        drop(held);
        recognition.push(&[]);
        assert!(ring.lock().unwrap().is_empty());
    }

    #[test]
    fn meter_reset_matches_fresh_meter_without_replacing_storage() {
        let mut meter = LevelMeter::new(8, 2);
        meter.push(&[100, -100, 200, -200, 300]);
        let pointer = meter.intervals.as_ptr();
        meter.reset();
        assert_eq!(meter.intervals.as_ptr(), pointer);
        let mut fresh = LevelMeter::new(8, 2);
        for samples in [&[10][..], &[-10, 20, -20][..], &[30, -30, 40, -40][..]] {
            assert_eq!(meter.push(samples), fresh.push(samples));
        }
        assert_eq!(meter.intervals.as_ptr(), pointer);
    }

    #[test]
    fn recognition_gap_clears_old_samples_after_snapshot_releases_lock() {
        let mut recognition = RecognitionBuffer::new(true, 16, 8, 4).unwrap();
        let ring = recognition.shared_ring();
        recognition.push(&[1, 2]);
        let held = ring.lock().unwrap();
        recognition.push(&[3, 4]);
        recognition.reset();
        recognition.push(&[5, 6]);
        drop(held);
        recognition.push(&[7, 8]);
        let mut out = Vec::new();
        ring.lock().unwrap().copy_latest(16, &mut out);
        assert_eq!(out, [5, 6, 7, 8]);
    }

    #[test]
    fn recognition_waiting_overflow_clears_pre_gap_history() {
        let mut recognition = RecognitionBuffer::new(true, 16, 2, 4).unwrap();
        let ring = recognition.shared_ring();
        recognition.push(&[1, 2]);
        let held = ring.lock().unwrap();
        recognition.push(&[3, 4, 5, 6]);
        drop(held);
        recognition.push(&[7, 8]);
        let mut out = Vec::new();
        ring.lock().unwrap().copy_latest(16, &mut out);
        assert_eq!(out, [5, 6, 7, 8]);
    }

    #[test]
    fn disabled_recognition_has_no_buffers() {
        assert!(RecognitionBuffer::new(false, 12, 4, 2).is_none());
    }

    #[test]
    fn enabled_recognition_flushes_samples_after_snapshot_unlocks() {
        let mut buffer = RecognitionBuffer::new(true, 12, 4, 2).unwrap();
        let ring = buffer.shared_ring();
        assert_eq!(ring.lock().unwrap().capacity(), 12);
        assert_eq!(buffer.waiting.capacity(), 6);
        buffer.push(&[1, 2]);
        let snapshot = ring.lock().unwrap();
        buffer.push(&[3, 4]);
        buffer.push(&[5]);
        assert_eq!(buffer.waiting, [3, 4, 5]);
        drop(snapshot);
        buffer.push(&[6]);
        assert!(buffer.waiting.is_empty());
        let mut out = Vec::new();
        ring.lock().unwrap().copy_latest(12, &mut out);
        assert_eq!(out, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn recognition_keeps_newest_waiting_samples_when_snapshot_stalls() {
        let mut buffer = RecognitionBuffer::new(true, 12, 4, 2).unwrap();
        let ring = buffer.shared_ring();
        let snapshot = ring.lock().unwrap();
        buffer.push(&[1, 2]);
        buffer.push(&[3, 4]);
        buffer.push(&[5, 6]);
        assert_eq!(buffer.waiting, [3, 4, 5, 6]);
        drop(snapshot);
        buffer.push(&[7, 8]);
        let mut out = Vec::new();
        ring.lock().unwrap().copy_latest(12, &mut out);
        assert_eq!(out, [3, 4, 5, 6, 7, 8]);
    }

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
    fn stereo_meter_opposite_phase_retains_energy() {
        let samples: Vec<i32> = (0..48_000)
            .flat_map(|i| {
                let x = if i % 48 < 24 { 4_194_304 } else { -4_194_304 };
                [x, -x]
            })
            .collect();
        let level = StereoLevelMeter::new(48_000)
            .push(&samples)
            .unwrap()
            .level_dbfs;
        assert!((level - -6.0206).abs() < 0.001, "got {level}");
    }

    fn stereo_square(frames: usize, left: i32, right: i32, dc: [i32; 2]) -> Vec<i32> {
        (0..frames)
            .flat_map(|i| {
                let sign = if i % 2 == 0 { 1 } else { -1 };
                [dc[0] + sign * left, dc[1] + sign * right]
            })
            .collect()
    }

    #[test]
    fn stereo_meter_in_phase_and_either_single_channel_have_calibrated_levels() {
        for (left, right, expected) in [
            (4_194_304, 4_194_304, -6.0206),
            (4_194_304, 0, -9.0309),
            (0, 4_194_304, -9.0309),
        ] {
            let samples = stereo_square(48_000, left, right, [0, 0]);
            let level = StereoLevelMeter::new(48_000)
                .push(&samples)
                .unwrap()
                .level_dbfs;
            assert!((level - expected).abs() < 0.001, "got {level}");
        }
    }

    #[test]
    fn stereo_meter_removes_each_channels_dc_without_losing_signal() {
        let samples = stereo_square(48_000, 2_097_152, -2_097_152, [5_000_000, -6_000_000]);
        let level = StereoLevelMeter::new(48_000)
            .push(&samples)
            .unwrap()
            .level_dbfs;
        assert!((level - -12.0412).abs() < 0.001, "got {level}");
    }

    #[test]
    fn stereo_meter_silence_and_unequal_dc_are_silent() {
        for dc in [[0, 0], [8_388_607, -8_388_608], [-8_388_608; 2]] {
            let mut meter = StereoLevelMeter::new(48_000);
            assert_eq!(meter.push(&[]), None);
            let samples = stereo_square(48_000, 0, 0, dc);
            assert_eq!(meter.push(&samples).unwrap().level_dbfs, SILENCE_DBFS);
        }
    }

    #[test]
    fn stereo_meter_full_scale_does_not_overflow() {
        let samples: Vec<_> = [8_388_607, -8_388_608, -8_388_608, 8_388_607]
            .into_iter()
            .cycle()
            .take(2 * 48_000)
            .collect();
        let mut meter = StereoLevelMeter::new(48_000);
        for _ in 0..3 {
            let level = meter.push(&samples).unwrap().level_dbfs;
            assert!(level.abs() < 0.001, "got {level}");
        }
    }

    #[test]
    fn stereo_meter_reads_every_quarter_second_including_startup() {
        let mut meter = StereoLevelMeter::new(48_000);
        let samples = stereo_square(96_000, 4_194_304, -4_194_304, [0, 0]);
        let mut readings = vec![];
        for (i, chunk) in samples.chunks(480).enumerate() {
            if let Some(reading) = meter.push(chunk) {
                let level = reading.level_dbfs;
                readings.push((i + 1) * 240);
                assert!((level - -6.0206).abs() < 0.001, "got {level}");
            }
        }
        assert_eq!(readings, (1..=8).map(|i| i * 12_000).collect::<Vec<_>>());
    }

    #[test]
    fn stereo_meter_keeps_only_one_second_and_returns_last_complete_interval() {
        let mut meter = StereoLevelMeter::new(48_000);
        let tone = stereo_square(48_000, 4_194_304, -4_194_304, [0, 0]);
        // Partial trailing silence must not change the last completed reading.
        let input = [&tone[..], &[0; 600]].concat();
        let level = meter.push(&input).unwrap().level_dbfs;
        assert!((level - -6.0206).abs() < 0.001, "got {level}");
        let level = meter.push(&vec![0; 24_000 - 600]).unwrap().level_dbfs;
        assert!((level - -7.27).abs() < 0.01, "got {level}");
        assert_eq!(
            meter.push(&vec![0; 72_000]).unwrap().level_dbfs,
            SILENCE_DBFS
        );
    }

    #[test]
    fn stereo_meter_preserves_frames_across_odd_and_interval_chunk_boundaries() {
        let samples = stereo_square(24, 4_194_304, -4_194_304, [0, 0]);
        for chunk_size in [1, 3, 7, 10, 25, 48] {
            let mut meter = StereoLevelMeter::new(8);
            let mut total = 0;
            for chunk in samples.chunks(chunk_size) {
                let before = total;
                total += chunk.len();
                let reading = meter.push(chunk);
                assert_eq!(reading.is_some(), total / 4 > before / 4);
                if let Some(reading) = reading {
                    let level = reading.level_dbfs;
                    assert!((level - -6.0206).abs() < 0.001, "got {level}");
                }
                assert_eq!(meter.push(&[]), None);
            }
        }
    }

    #[test]
    fn stereo_meter_reset_discards_window_partial_interval_and_half_frame() {
        let mut meter = StereoLevelMeter::new(8);
        meter.push(&stereo_square(9, 4_194_304, -4_194_304, [0, 0]));
        meter.push(&[8_388_607]);
        meter.reset();
        assert_eq!(meter.push(&[123, -456, 123]), None);
        assert_eq!(meter.push(&[-456]).unwrap().level_dbfs, SILENCE_DBFS);
        let level = meter
            .push(&stereo_square(8, 4_194_304, -4_194_304, [0, 0]))
            .unwrap()
            .level_dbfs;
        assert!((level - -6.0206).abs() < 0.001, "got {level}");
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
