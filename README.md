# esp-shazam

An always-listening Shazam box. Rust firmware for the Ai-Thinker ESP32-A1S Audio Kit listens to its LINE IN jack, fingerprints the audio on the board (SongRec's algorithm), asks Shazam, and shows the song at `http://esp-shazam.local/`, as JSON at `/api/state`, and on MQTT topic `esp-shazam/now_playing`.

- Design: `docs/superpowers/specs/2026-10-03-esp-shazam-design.md`
- Check everything: `scripts/check`

Licensed GPL-3.0-or-later (the fingerprint code comes from SongRec).
