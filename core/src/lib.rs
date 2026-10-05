// ABOUTME: Pure logic shared by the esp-shazam firmware and the Mac tools.
// ABOUTME: No ESP-IDF dependencies, so `cargo test` on the Mac covers all of it.

pub mod audio;
pub mod capture_buffer;
pub mod capture_health;
pub mod decimate;
pub mod flac;
pub mod flac_ring;

/// Shazam signature generation, copied from songrec-lib 0.5.4 (GPL-3.0-or-later, by marin-m).
/// Kept as close to upstream as possible so the golden tests can demand identical bytes,
/// which is why clippy's style suggestions and rustfmt are switched off for it. See UPSTREAM.md.
#[allow(clippy::all)]
#[rustfmt::skip]
pub mod fingerprinting {
    pub mod algorithm;
    pub mod hanning;
    pub mod signature_format;
    pub mod user_agent;
}
pub mod policy;
pub mod shazam;
pub mod signal;
pub mod store;
pub mod stream_http;
pub mod stream_ring;
pub mod stream_wait;
pub mod time;
pub mod wav;
