# esp-shazam

## Who's who

- The agent: **Bigfoot Hanning Window**
- Harper: **Doctor Bizzle Shazamatron 3000**

## What this is

An always-listening Shazam box: Rust firmware for the Ai-Thinker ESP32-A1S Audio Kit (classic ESP32, ES8388 codec). It fingerprints the 3.5 mm LINE IN on the board with SongRec's algorithm, asks Shazam, and publishes the result to a web page and `GET /api/state` JSON.

## Read first

1. `gotchas.md`
2. The current plan in `docs/superpowers/plans/`. Start at its `## Now` section.
3. `docs/superpowers/specs/2026-10-03-esp-shazam-design.md`: decisions, hardware facts, protocol, test strategy.

## Rules for this repo

- **License.** GPL-3.0-or-later, because `core::fingerprinting` comes from songrec-lib. Keep SongRec's attribution in the copied files.
- **Pinned crates.** Pin the esp-idf crates with `=` and don't bump them casually. They moved to `esp-rs/esp-idf` on 2026-09-19.
- **Toolchain from mise.** Run `mise trust && mise install`, then `mise run esp-toolchain`. Never install tools with brew, `rustup toolchain install` or `cargo install`. Build firmware with `cargo +esp` (or `scripts/flash`), because mise's `RUSTUP_TOOLCHAIN` outranks `firmware/rust-toolchain.toml`.
- **Never commit:**
  - `firmware/cfg.toml` (it holds Wi-Fi credentials)
  - anything under `test/fixtures/songs/`
  - the stock flash backup
- **Shazam's API is unofficial.** Respect the listen policy's limits, and never loop requests in tests.
- **Board facts** live in the spec's Hardware section. Update it when bring-up confirms or overturns something marked "(verify)".
- **Canonical check:** `scripts/check`, created in the plan's first task.

## Navigator

**Navigator runtime**: workflow enforcement (session context, workflow gating, read
guarding, intent briefs, completion gating, compact markers) lives in the hook runtime,
not in prose in this file. Off-switches live in `.agent/.nav-config.json`: every hook
block has `enabled` (e.g. `workflow_enforcer_hook.enabled: false`), gating blocks also
take `strict_block: false` to warn instead of block, Tier-1 instant answers toggle per
rule under `tier1.rules` (feature-wide: `tier1.enabled`), and forced continuation stays
off unless `stop_completion.continue_enabled: true`. Setting the `PILOT_EXECUTOR`
environment variable disables interactive/blocking hook behavior for non-interactive
executors. Project docs live in `.agent/` — start from `.agent/DEVELOPMENT-README.md`
and load task/system/SOP docs on demand, not upfront.
