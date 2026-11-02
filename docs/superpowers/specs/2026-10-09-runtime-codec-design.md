# Configurable MP3 and FLAC

Doctor Biz requested both codecs in one configurable firmware, retaining the
existing buffer. This replaces the compile-time-only codec choice. The board
stays on the new iteration image, with MP3 selected after testing. No automatic
FLAC rollback, volume changes, push or merge.

## User behavior

Doctor Biz also requested a Settings page. Add `/settings` for Format: MP3 /
FLAC, Startup buffer: Low 1s / Medium 4s / High 8s, and Autoplay. The main page
keeps playback status, destination selection and Play/Stop, with navigation
between pages. Both pages use the same saved settings API. Both choices persist independently. A change during
playback applies on the next Play, preserving the current session and frozen
queue metadata. Manual Stop still wins. Buffer settings remain source-history
targets, not guaranteed speaker latency.

A format setting can apply immediately when Sonos is stopped and no compressed
HTTP listener is active. Otherwise it remains pending until the next Play.
The page/API distinguish selected and active format. A direct HTTP request never
changes the global codec: only the active compressed route streams. Both codecs
are present in the same firmware, but only one encoder executes at a time.
WAV remains available as raw diagnostic output.

A deliberate Play can end prior compressed debug listeners when changing format;
they must end explicitly, never receive bytes from the replacement codec. A frame
already in flight may finish under its existing absolute deadline. Buffer history
starts fresh after the format commit and fills to the selected whole-frame delay.

## Runtime ownership

One encoder worker owns the active codec. Switching occurs at a block boundary
through a bounded control request. Prepare a correctly sized replacement ring
and codec before committing. Allocation failure leaves the current codec/ring
usable and reports a clear failure; it must not publish a mismatched Sonos queue.
Temporarily retaining two allocated codec states for rollback is acceptable;
executing both encoders concurrently is not. Do not allocate 500 FLAC-size slots.

The active epoch binds format, ring and cancellation state. Admission obtains a
consistent epoch, and old readers observe retirement on commit. Old copied frames
may finish their bounded write; their ring cannot be reused for the new codec.
No capture-path changes, reader-held capture locks or unbounded switch waits.
A timed-out or canceled request loses authority to commit: the worker rechecks
request cancellation immediately before commit. A canceled Play must not later
publish/start a Sonos session. Idle switching atomically arbitrates its no-listener
predicate against admission; if a listener wins during preparation, defer the
idle switch and keep the old epoch.
Temporary memory overlap and repeated switches require hardware measurement.

FLAC retains 141 slots/12.032s when recognition is disabled, MP3 500 slots/12s.
Low/Medium/High select 12/47/94 FLAC frames or 42/167/334 MP3 frames. The format
profile supplies block size, frame bytes, MIME, header/renumbering and write budget.
Recognition-enabled builds must refuse MP3 selection with an explicit error;
do not silently disable recognition. Keep the verified priority policy (FLAC7,
MP3 8) and an adequately sized worker stack without concurrent worker lifetimes.

Saved format defaults to FLAC if missing, preserving the established default.
During device validation explicitly select MP3 and leave that selection saved.
A new field with a default is one schema, not a parallel legacy code path.
Sonos QueueSnapshot captures codec and buffer together; URLs, content type and
report validation use that immutable snapshot, not current settings.

## Verification

Write real failing tests first for format persistence/validation, active routing,
queue immutability, both codecs' preset math, switch/epoch behavior, allocation
rollback and independent decoding before/after a switch. UI tests exercise actual
page code; the existing minimal DOM tests are unit tests, not browser E2E.

Run canonical scripts/check and build unified firmware. Independent Sol review
covers concurrent switch/admission, deadlines, resource ownership and Stop.
On hardware test MP3→FLAC→MP3 with real decode counts, no mixed connection bytes,
settings persistence over reboot and saved buffer behavior. Check capture/heap/
encoder telemetry across switching and repeat cycles to detect retained rings.
Test Sonos queue metadata when feasible without volume changes. Preserve existing
two-listener delivery failure evidence; runtime selection is not its fix.
