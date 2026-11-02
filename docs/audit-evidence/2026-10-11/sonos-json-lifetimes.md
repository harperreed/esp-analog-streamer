# Sonos JSON lifetime correction

Three parsed JSON values outlived their last use: the validated getInfo response,
the createSession response after copying its session ID, and a request body after
encoding its owned wire bytes. Explicit drops release these values before later
TLS writes. The six-line production change adds no protocol or recovery behavior.

## Hardware observations

The temporary diagnostic wrapped the pinned TLS allocator, called the original
allocator unchanged, and recorded the first failed 4437-byte allocation. It
tracked owned and queued WebSocket payload capacity separately. Logging occurred
outside the allocator. The record samples counters and heap information
separately; it is not an atomic snapshot of the whole system.

| Candidate | Observation | Result |
| --- | --- | --- |
| Allocation probe | First failure: 1587 internal bytes free, largest block 1216; tracked owned and queued payload counts both zero. | Native playback failed. |
| Drop getInfo | Free memory rose from 1675 to 16163 bytes across the drop; largest block rose from 1280 to 7680. A native FLAC listener joined. | Later TLS allocation failed with 10883 bytes free, largest block 3712. |
| Drop all three values | getInfo released 14476 bytes; session response released 584; createSession and loadCloudQueue request bodies released 504 and 592. | Stop's later TLS allocation failed with 11019 bytes free, largest block 3712. |

The first correction removes demonstrated unnecessary memory pressure. It does
not establish reliable playback or fix the later contiguous-allocation failure.
The allocator still requests 4437 internal bytes, including for a small
WebSocket header. More total free memory alone cannot satisfy that request when
the largest block is 3712 bytes.

## Failed harnesses and cleanup

All three trials remain failed. The getInfo-only driver rejected an empty track
URI during buffering. The all-drops driver then hit HTTP 404 while reading a
queue before publication. Neither error proves a codec failure. Both trials also
failed their initial cleanup when the board could not send Stop over TLS.

Separate cleanup verified the current coordinator and exact published track URI
before issuing SOAP Stop. Both subsequent cleanup receipts passed. The board was
returned to stopped MP3/Medium with autoplay and recognition disabled. The source
and UART reader were joined. Original failed results were not rewritten.

Private raw records remain under `target/kata-hardening/tls-failure-receipt/`:

| Trial | Diagnostic ELF SHA-256 | Result JSON SHA-256 |
| --- | --- | --- |
| native-attempt | `0094a8b271ec4f8869e4fe7128eeacb7c73a0ba2e4e7b53fdc652745413462ac` | `2689698c3f7853d831c9b08527f0ca31ad32522f58f15fd33fa94c4b2ef9f12d` |
| drop-attempt | `8d7fc770ea3b78ee34d31ebe4b6e7e2221dc2965c57e70f3b5e13223c2e9fe44` | `887db82d20bbf8046a9c0a798b8102e786fa3ebe4ea6867bdd5b77834ea943dd` |
| all-drops-attempt | `70082c2626b550951905bd8ef9fc7858cd5da91dcfbbca7d3e852cfab28797ec` | `4bd40174c822661147907d1cc44a0011038f87423920d988c4b5013884a62fce` |

Each driver attested the running ELF and boot identity. Diagnostic patches and
the probe source are archived alongside the receipts. The production change
removes the wrapper, temporary tracking, heap logging and probe module.

## Scope of verification

The 32 host Sonos adapter tests pass, but exclude the device module containing
these drops. They are not hardware validation of the fix. An independent source
review confirmed that the copied session ID and encoded bytes own their data and
that all drops follow the last use of each JSON tree. The hardware observations
above establish the memory release in diagnostic images; they do not certify
the final image's playback, recovery, or endurance. No Kata closes from this fix.
