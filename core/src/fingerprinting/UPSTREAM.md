# Upstream: songrec-lib 0.5.4

Source: https://static.crates.io/crates/songrec-lib/songrec-lib-0.5.4.crate, `src/fingerprinting/`.
License: GPL-3.0-or-later, by marin-m (https://github.com/marin-m/SongRec).

Files: algorithm.rs, signature_format.rs, hanning.rs, user_agent.rs.

Changes for esp-shazam (the golden tests in core/tests/golden.rs prove output bytes are unchanged):
1. algorithm.rs: removed `make_signature_from_file` (needs rodio) and its imports.
2. algorithm.rs: the two 256 x 1025 rings are `Vec<[f32; 1025]>` (one 1 MB allocation each)
   instead of `Vec<Vec<f32>>` (256 allocations of 4 KB). ESP-IDF keeps allocations under 16 KB in
   internal RAM; one big allocation lands in PSRAM instead.
3. signature_format.rs: deprecated `base64::encode/decode` replaced by base64 0.22's `Engine` API.
