// ABOUTME: Computes per-channel integer extrema and moments for the real settling measurement.
// ABOUTME: Shares the allocation-free calculation with host numeric regression tests.

#[derive(Clone, Copy, Default)]
pub struct ChannelStats {
    pub min: i32,
    pub max: i32,
    pub sum: i64,
    pub sum_squares: u64,
}

impl ChannelStats {
    /// Caller supplies at most 240 whole stereo frames of signed 24-bit samples,
    /// converted by production samples_from_i2s_slots; channel is 0 or 1.
    /// Reads the borrowed slice without allocation. Empty reads report zero stats.
    pub fn measure(samples: &[i32], channel: usize) -> Self {
        let mut stats = Self {
            min: i32::MAX,
            max: i32::MIN,
            ..Self::default()
        };
        for &sample in samples.iter().skip(channel).step_by(2) {
            stats.min = stats.min.min(sample);
            stats.max = stats.max.max(sample);
            let value = i64::from(sample);
            stats.sum += value;
            // At most 240 signed 24-bit values: sum of squares is <= 240 * 2^46.
            stats.sum_squares += (value * value) as u64;
        }
        if samples.is_empty() {
            stats.min = 0;
            stats.max = 0;
        }
        stats
    }
}
