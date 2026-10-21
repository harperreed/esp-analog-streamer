// ABOUTME: Build-time settings from firmware/cfg.toml (gitignored): Wi-Fi, codec gain, the Shazam on/off flag, listen-policy timings.
// ABOUTME: Defaults match the spec; cfg.toml.example lists every key.

use esp_shazam_core::policy::PolicyConfig;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_pass: &'static str,
    #[default("esp-shazam")]
    hostname: &'static str,
    // ES8388 PGA register value: 0 is 0 dB, right for line level (see codec::setup_line_in).
    #[default(0)]
    pga_gain: u8,
    // Shared secret for POST /api/ota (header X-Ota-Token). Empty disables over-the-air updates.
    #[default("")]
    ota_token: &'static str,
    // false skips the recognizer thread: no fingerprinting, no Shazam or TLS traffic. Everything else runs.
    #[default(true)]
    shazam_enabled: bool,
    #[default(-50.0)]
    quiet_threshold_dbfs: f32,
    #[default(8)]
    search_interval_secs: u32,
    #[default(30)]
    slow_search_interval_secs: u32,
    #[default(30)]
    recheck_interval_secs: u32,
    #[default(3)]
    clear_after_misses: u32,
    #[default(60)]
    clear_after_quiet_secs: u32,
    #[default(300)]
    rate_limit_backoff_secs: u32,
}

/// The listen policy with cfg.toml's settings. The miss count that slows searching (5) isn't a spec
/// setting, so it keeps its default.
pub fn policy_config() -> PolicyConfig {
    let secs = |s: u32| u64::from(s) * 1_000;
    PolicyConfig {
        quiet_threshold_dbfs: CONFIG.quiet_threshold_dbfs,
        search_interval_ms: secs(CONFIG.search_interval_secs),
        slow_search_interval_ms: secs(CONFIG.slow_search_interval_secs),
        recheck_interval_ms: secs(CONFIG.recheck_interval_secs),
        clear_after_misses: CONFIG.clear_after_misses,
        clear_after_quiet_ms: secs(CONFIG.clear_after_quiet_secs),
        rate_limit_backoff_ms: secs(CONFIG.rate_limit_backoff_secs),
        ..PolicyConfig::default()
    }
}
