# Configurable startup history

Doctor Biz selected Low 1 second / Medium 4 seconds / High 8 seconds.
Medium is the default. Save the setting through the existing Sonos configuration
and apply it on the next Play. An active queue keeps its original media URL.

The existing 141-slot FLAC ring supplies 12, 47 or 94 complete frames, rounded
up to 1.024, 4.010667 or 8.021333 seconds at 48 kHz. This adds no ring allocation.
The ordinary FLAC URL remains live; WAV and capture precision stay unchanged.
These values describe source history, not guaranteed Sonos playback latency.

Recognition-enabled firmware keeps its live stream and explicitly rejects
buffered routes/settings: its 24-slot history cannot support all three presets.
The UI explains why its selector is unavailable in that profile.

## Acceptance

- [x] Shared preset values, migration from saved settings without a preset,
  immutable per-session URLs, non-canceling saves and UI confirmation tests.
- [x] Retained generation/cursor selection, strict buffer queries, bounded fill,
  producer stalls, resets, socket failures and complete-request half-close tests.
- [x] Exact Claxon and FFmpeg decoding of all presets after ring wrap.
- [x] Sol spec and quality reviews of both implementation lanes.
- [x] Canonical `scripts/check`: all 20 stages pass.
- [x] Recognition-enabled compile; original build configuration restored exactly.
- [x] Real board browser settings, persistence, next-Play behavior and Stop.
- [x] Real board decoded PCM comparisons for all three presets.
- [x] Record hardware limits and commit the source as `9964baf`.
- [x] Record evidence and Kata receipts: `bzgt` closed; `dctv` and `dp08`
  updated with the bounded pass and capture failure respectively.

Startup waits have one absolute preset-duration-plus-two-second budget and a
separate two-second producer-progress budget. Definitive socket failures end
the wait; ordinary FIN also permits a valid request half-close and cannot prove
that a client stopped reading. No socket work holds the ring mutex.

MP3 remains feasibility Kata `e8hd`, not part of this implementation. Sonos
supports it, but encoder CPU, capture interference, quality and joining behavior
need measurements on the original ESP32. Related continuity Kata `dctv` also
retains recovery/resynchronization and receiver-cushion validation work.

Implementation receipts: `.superpowers/sdd/buffer-presets/`. Raw checks and
private hardware artifacts: `target/buffer-presets/`. Compaction count: 1.

Hardware results: all three presets matched exact PCM at their rounded targets.
High survived reset and a cold request waited 8.22 seconds for retained history.
Medium and High played on real Sonos; saving High preserved the current Medium
queue, next Play used High, and manual Stop passed. Source behavior is accepted;
capture protection is not. Initial Sonos setup added nine queue overflows, one
restart, 240 software-dropped frames and 24,000 settling-discarded frames. See
`docs/audit-evidence/2026-10-08/buffer-presets.md` and its JSON receipt.
Next reliability work remains in `dp08`, `dctv` and the release gate; this feature
does not claim those issues resolved. MP3 remains the separate `e8hd` benchmark.
