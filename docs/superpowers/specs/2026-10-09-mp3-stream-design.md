# MP3 streaming prototype

Doctor Biz explicitly approved starting the prototype after the measured Shine
benchmark and corrected a premature stop. This is authorized implementation and
hardware validation; no repeated design approval is needed. The benchmark is
evidence, not an acceptance substitute. Kata `e8hd` tracks the work.

An opt-in `mp3-stream` firmware feature selects one shared 48 kHz stereo,
320 kbps encoder instead of FLAC. The default build retains FLAC. Concurrent
encoding failed the measured CPU budget, and runtime switching would add an
unneeded lifecycle transition before validating the codec. WAV remains the raw
diagnostic route. Recognition plus MP3 is outside the measured CPU budget and
must fail at build time rather than silently disable recognition.

Use pinned Shine `ab5e3526b64af1a2eaa43aa6f441a7312e013519` in a small shared
crate, with upstream notices, license and local patches recorded. Patch unchecked
allocation/growth paths, use per-instance allocator callbacks with strict PSRAM
for the C encoder state and encoded history on the board, and reject malformed
output. The 4,608-byte PCM workspace uses fallible heap construction through the
normal allocator; it must not travel by value through the main task stack. The benchmark's global allocator
hooks and abort policy are not production code. Preserve current 24-to-16-bit
conversion; do not silently compensate for the measured gain difference.

The encoder reads the actual atomic capture ring, publishes complete 960-byte
MP3 frames representing 1,152 stereo sample frames, and shares one bounded
history among listeners. Reuse the existing encoded ring, admission, generation,
deadline and socket-cleanup protocols. MP3 needs no FLAC stream header or frame
renumbering. Capture overrun resets the codec and history generation; old streams
end explicitly. Failed codec initialization must return an error with no partial
server startup. Failed encoding must invalidate old history and stop publishing.

Five hundred slots retain exactly 12 seconds, with 480,000 payload bytes.
Low/Medium/High retain whole-frame offsets of 42/167/334 frames, or
1.008/4.008/8.016 seconds. Settings remain 1/4/8-second targets. These are source
history offsets, not a claim about speaker latency. One active format source
determines HTTP path, MIME type, Sonos queue URLs and report matching. Inactive
compressed paths must refuse, never return a body of a different codec.

Success requires real host encoding/FFmpeg decode and join tests, allocation
failure cleanup, both build profiles, bounded ring/reset behavior and unchanged
Sonos Stop/source-ownership behavior. Hardware tests must use real capture,
one/two MP3 listeners, startup presets, slow readers, TLS and real Sonos playback.
Record actual decoded samples, capture/encoder progress and faults. Do not infer
content continuity from timestamps alone. Preserve a known production ELF and
restore stopped/autoplay-off/Medium settings after tests; never change volume.

MP3 is lossy. Measured delay/tail handling, roughly +1 dB gain, join warmup,
occasional half-second encoder stalls, license obligations and long-soak gates
remain explicit risks until resolved or measured. Implementing this opt-in
prototype does not authorize changing the default codec or calling it release-ready.
