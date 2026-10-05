# Validate the physical network path

Doctor Biz authorized autonomous completion of the kata audit with sol-6.1
subagents. Shazam stays off. No system routes, VPN settings, or firmware network
limits change in this phase.

## Evidence and scope

The default route to the board uses utun4 with MTU 1280. Repeated two-listener
runs lost a listener to a frame deadline or ring overrun. A real API socket bound
to en0 answered in 67 ms; two en0-bound stream sockets then held for the full
60-second host bound. Complete-frame CRC and decoded-prefix verification remain
with the hardware controller. This is path sensitivity, not a proven tunnel or
firmware root cause. Physical input is quiet ADC/player audio, not full-scale
broadband noise.

## Implementation

- `capture_audit`: a small macOS TCP relay with two loopback-only ephemeral
  listeners, fixed upstream board ports 8000 and 80, and upstream sockets bound
  to the requested interface before connect. Bound connection count and buffers;
  forward real bytes unchanged, drain queued bytes before half-close, and close
  owned sockets on exit. Emit endpoint/interface metadata, never payload logs.
- `ring_spec`: add validated `--stream-port` and `--state-port` options to the
  existing fault and soak harnesses. Defaults preserve their current endpoints.
  Update every real connection and decoder URL; retain all deadlines and checks.
- Root: review integration and run the canonical check. Each implementer reviews
  the other's change before commit. The hardware controller alone uses UART,
  flashes firmware, and operates the board.

## Verification

1. Real loopback host regressions for exact relay bytes, half-close behavior,
   connect failure, bounded buffers and cleanup. Exercise nondefault harness ports.
2. Repeat the default-route observation to compare with en0; retain failures.
3. Run the existing strict internal-pressure test through the relay. Require real
   refusal, checked cleanup, sampled floor, capture continuity and both full
   ten-second decoded recoveries before closing nr6z.
4. Restore production firmware. Run the unchanged decoder/capture/heap acceptance
   through explicit relay endpoints, then the long soak if the bounded run holds.
5. Run `GIT_CONFIG_GLOBAL=/dev/null scripts/check` after source changes. Record
   ignored tests and any remaining hardware limits. Keep user-owned `.gitignore`
   and `.kata.toml` untouched.

Relay evidence must say **en0-relayed**. An added TCP hop and buffering cannot
establish the default-route slow-reader envelope. Successful direct-path playback
does not close the tunnel-path reliability or physical worst-case input gaps.

State: implementation in progress. Compaction count inherited from audit: 5.
