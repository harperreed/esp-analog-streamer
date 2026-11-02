// ABOUTME: Checks that a recognition snapshot contains enough audio after capture resets.
// ABOUTME: Defers short snapshots without recording a Shazam answer or extending backoff.

use esp_shazam_core::policy::{Policy, PolicyConfig};

/// Fingerprints consume mono 16 kHz PCM. Preserve shorter initial intervals,
/// capped at the default eight seconds rather than the entire history.
pub fn admit(policy: &mut Policy, config: PolicyConfig, samples: usize) -> bool {
    let default_samples = PolicyConfig::default().search_interval_ms * 16;
    let configured_samples = config
        .search_interval_ms
        .min(PolicyConfig::default().search_interval_ms)
        * 16;
    // The pinned generator first runs peak recognition after 46 chunks of 128
    // samples. Zero or shorter intervals cannot supply even that first attempt.
    let minimum_samples = if configured_samples < 46 * 128 {
        default_samples
    } else {
        configured_samples
    };
    if (samples as u64) < minimum_samples {
        policy.defer_ask();
        false
    } else {
        true
    }
}
