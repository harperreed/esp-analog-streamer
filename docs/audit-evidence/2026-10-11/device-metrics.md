# Device metrics checkpoint

This historical checkpoint is followed by the [completed metrics acceptance](live-listener-metrics.md).

amqe remains open. The numeric endpoint, bounded host collector and separate
41-panel dashboard are implemented and checked. Live individual-listener
observations and rendered dashboard acceptance remain incomplete.

## Verified implementation

`GET /api/metrics` copies existing low-priority owner observations through a
nonblocking cache. It serializes after releasing the guard, caps JSON at 4096
bytes, and returns a fixed 503 body when unavailable. Capture/ISR work does not
publish metrics. The cache preserves source timestamps and reports missing
families as null. Boot/build/settings metadata contains no peer, credential or
audio data. The running application descriptor's ELF digest matched the flashed
artifact on both observed boots; boot tokens differed after reset.

`scripts/device-metrics` polls the numeric endpoint with a deadline and serves
Prometheus and identity metadata on host loopback. It bounds body sizes,
cardinality and concurrency. Source freshness and HTTP freshness remain separate;
generation counters are gauges. The dashboard preserves the existing Navigator
artifacts and configuration.

Review caught two integration errors and regressions now cover both: accepted
and published sample counts come from separate reads, so publication may exceed
the earlier accepted observation; an encoder epoch change invalidates backlog
derived from the retained previous output receipt. Neither case should reject a
valid whole response or invent new audio production.

Canonical `scripts/check` run `target/check-logs/run.9Glqmfvm` completed with
27 passing stages and exit 0. Host bounds were Cache 664, Snapshot 656 and Response
736 bytes; maximal populated JSON was 3735 bytes. Target size assertions passed.
The earlier run `run.zD2jFnih` remains failed: the route did not read its error
variant payload, producing a target warning. Explicit error matching fixed it.

## Bounded live evidence

The exact checked ELF was
`589e1dcf9c58655960765ad332ebe0f753c2d64bd41e030ed85b9bff916c285e`.
It stayed installed with MP3/Medium, stopped playback, autoplay off and recognition
off. The source was the recorded mono-correlated noise fixture, sent through the
confirmed front jack without mixer writes. This was not independent stereo noise.

Two HTTP MP3 clients remained connected through 40-second collector OFF, ON and
OFF phases. Each received 4,844,160 payload bytes before planned shutdown. The
interior counter spans covered about 30.5 seconds per phase and had zero changes
in all nine checked capture-fault counters. Sampled minimum internal free memory
was 48,407 bytes and largest free block 47,104 bytes in each phase. These samples
do not establish zero overhead, worst-case memory, or sample-exact continuity.

A planned listener close generated a fresh 4,855,680-byte sent receipt. That exact
value reached Prometheus and Grafana's datasource query. Sender bytes can exceed
received bytes when sockets close; the difference is not an audio-loss finding.
Rapid HTTP reads advanced served time while retaining the same capture observation.
A later real scrape kept HTTP availability true but marked the completed-listener
family stale and omitted its expired values. One live device body was 2715 bytes.

The temporary services used verified Grafana 13.2.3 and Prometheus 3.13.4 binaries,
private state and loopback ports. Grafana provisioned all 41 panels. Existing
Docker socket access was unavailable; no permission, group or existing service
configuration changed. Linux host processes shared loopback, avoiding the mismatch
between Docker host-gateway addresses and a loopback-only exporter. Owned services,
audio source, collector, sockets and UART reader were stopped and reaped.

## Remaining original scope and risks

- Live individual-listener lag/write measurements are missing. The current
  listener fields explicitly describe the latest completed receipt.
- Browser rendering is unverified. Sandboxed Chromium failed to start; no sandbox
  was disabled. Backend query success is narrower than a rendered-panel pass.
- The metrics owner's lowest observed stack watermark was 824 bytes. Emitted
  Xtensa assembly showed entry frames of 656 bytes for capture logging, 688 for
  media-progress export, 304 for encoder rendering and 1504 for the metrics HTTP
  handler. These exclude callees and interrupt depth. HTTP runtime headroom and
  saturation acceptance still need measurement before shipping.
- Startup still recorded 16 queue overflows, 480 software-dropped frames and two
  restarts before the sampled steady spans. Keep 93hm and broader release gates
  open; this work does not establish startup reliability.

The next implementation should add bounded reusable listener observation slots
without peer-derived identity, then verify them under two listeners and slot reuse.
Existing admission slots contain only a count; diagnostic frame-state slots are
permanently reserved and must not be relabeled as reusable production slots.
Prefer a small design and reuse the copied snapshot by reference during
serialization before considering schema duplication or larger memory bounds.

Structured results and private artifact digests are in
[device-metrics.json](device-metrics.json). Raw UART, audio and temporary service
credentials remain under ignored `target/kata-hardening/device-metrics/`.
