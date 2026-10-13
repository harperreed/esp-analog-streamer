# Report the actual stream write budget

Commit `261fd12` corrects timeout evidence without changing enforced deadlines,
buffer sizes or playback policy. The October 11 failed contention trial remains
failed; this change does not locate or repair its transport stall.

That trial's terminal write timed out after 782 ms. Its log printed the nominal
8021 ms socket allowance, although media age had already shortened the deadline.
The owner's 3.535870-second maximum write and 3.528569-second maximum poll were
lifetime maxima, not measurements of that terminal write.

Compressed timeout receipts now separate nominal milliseconds, remaining
microseconds and the actor's absolute boot-clock deadline. WAV retains its
original Instant deadline and full budget; it reports no boot deadline. Reports
still use the bounded deferred queue. No new per-frame logging or allocation was
introduced.

The retention parser validates the effective budget. It rejects an early timeout,
a shortened WAV budget, missing fields and malformed numeric suffixes. Tests
exercise the actual production telemetry, media-age calculation and shell script's
Python implementation. The numeric-suffix regression failed before correction.

Independent Sol 6.1 review accepted the final five-file change after correcting
three parser findings. `scripts/check` passed all 27 stages in
`target/check-logs/run.83iaZywQ`, exit 0, status complete, including firmware build,
IRAM placement and warning checks. The earlier passing run.Pl8ARSUO predates the
last parser correction and is not the final-source receipt.

No device flash, audio playback or hardware timeout observation occurred during
this correction. The installed board still needs a relevant hardware trial to
demonstrate the new receipt. Keep coexistence and recovery acceptance open.
