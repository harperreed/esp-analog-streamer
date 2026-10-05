# Validate the physical network path

Doctor Biz authorized autonomous completion of the kata audit with sol-6.1
subagents. Shazam stays off. No system routes or VPN settings change. Temporary
firmware experiments require verified generated settings and preserved controls;
no permanent network-limit change follows from one observation.

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

## Updated evidence and next experiment

Both en0 files have 641 sequential CRC-valid complete frames and decode 54.6987 s
of source audio. Their final partial frames were cut by the host stop. Receipt
was not real-time throughout: first/middle/final 20-second windows delivered
17.7493 / 22.1867 / 14.7627 s of PCM. The output observer counted 128 actual
single-pbuf driver allocation failures. The default-route repeat again lost
both listeners, with 114 such failures. Capture counters stayed flat.

Root approved one cache-64 comparison using the same en0 direct-file collector,
without relay buffering. This revisits capacity under the higher delivered load;
the earlier tunnel cache comparison remains inconclusive. Keep static TX 16,
MSS 1440, send buffer 46080, source, observer and deadlines unchanged. Verify boot
configuration and heap placement, per-window PCM rate and driver errors. Restore
cache 32 if the comparison gives no meaningful benefit. No MSS experiment runs
while this comparison is active.

State: relay review fixes and cache comparison in progress. Compaction count
inherited from audit: 5. Production restoration and strict decoded recovery are
still required.
