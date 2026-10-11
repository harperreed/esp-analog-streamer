# Smaller TLS transmit records

The preceding [JSON lifetime correction](sonos-json-lifetimes.md) released unused
memory, but a later 4437-byte internal TLS allocation failed when the largest
free block was 3712 bytes. The candidate sets
`CONFIG_MBEDTLS_SSL_OUT_CONTENT_LEN=2048`, reducing that record allocation to
2389 bytes with the pinned configuration. This is a transmit-record limit, not
an increase in buffering or a change in certificate validation.

Pinned ESP-IDF 5.5.5 permits outgoing record sizes from 512 through 16384.
`esp_mbedtls_write` splits larger application writes at this limit. Handshake
messages also have output-size limits, so this source inspection alone cannot
prove compatibility. The generated configuration confirms incoming size 16384,
dynamic buffers, internal allocation and the full certificate bundle unchanged.

## Native Sonos test

The final native trial passed 15 seconds of repeated Playing and exact published
track readbacks, followed by ownership-checked Stop, configuration restoration,
and source/UART cleanup. The same build and boot were attested before and after.
The native FLAC listener sent 247 frames before its recorded departure. This
test does not verify exact decoded samples, audible output or sustained recovery.

WebSocket read errors and the listener's connection reset were logged around
Stop. The receipt retains them; it does not claim error-free transport teardown.
No 4437-byte allocation failure appeared in this trial.

Two earlier attempts remain failed. Both queried the queue before publication;
Sonos initially retained the original stopped radio URI. One driver incorrectly
allowed only an empty track, and the next incorrectly assumed the coordinator
queue URI already existed. The final driver waits on the exact saved starting
media or the matching coordinator queue within its existing startup deadline.
Neither waiting case grants ownership. Stop still requires the actual published
track URI and a fresh coordinator check. Separate owned cleanup succeeded for
both failed attempts.

Private receipts are under `target/kata-hardening/tls-out-2048/`:

| Artifact | SHA-256 |
| --- | --- |
| Production ELF | `4acdcb1ce0b367e8b5c007d5c4e4d01336ea57abea188a7701bc8b900eb7ac53` |
| native-attempt/result.json, failed | `7146af2bad0e745b39bc4cdcf5562a5e8ce7170985d13871320c914e24c1f752` |
| native-repeat/result.json, failed | `640f7801d312dbc396a2c2f7d0811a479b0100392a6b62a12a67667fe0f519ce` |
| native-final/result.json, passed | `55917588dc3d14f1ff714b887c95b34060457ff373d4ad8e3ce09325d01aaec1` |
| Final native driver | `bfe5961c1ce25008d916a2e64d359bc93f499c4a48f6c66ba52fde1f4eee707c` |

The final native boot token was `53276c70dc38b6b1549c2dc267a3d562`.
The driver archives, effective SDK configuration and raw UART are retained.

## Shazam HTTPS compatibility

A throwaway example reused the production fingerprint generator, request builder,
HTTPS client, certificate bundle and response parser. The device sent a genuine
14027-byte request and received a parsed Miss in 768 ms. UART records certificate
validation before the PASS. This exercises both the handshake and a request
larger than the outgoing record limit; it does not reproduce simultaneous
recognition/playback heap pressure.

The diagnostic ELF SHA-256 is
`5a3a8dae936ceefee87670beedf9e07bde5302b9f971cb8813377964b17a30c0`.
The source and ELF are archived beside `shazam-uart-repeat.log`. The example was
removed after the test. An earlier interactive monitor was suspended by terminal
job control and produced no device result; a subsequent monitor could not open
the still-owned port. Both logs remain. The successful noninteractive monitor
was explicitly terminated after PASS, releasing UART before restoration.

This change does not establish ownership-safe reconnect, eliminate capture gaps,
or complete any Kata's recovery acceptance. Native testing used one FLAC listener;
two-listener recovery, takeover, interruption and endurance gates remain open.

The restored unified firmware is byte-identical to the tested production ELF.
Fresh API reads confirm stopped MP3/Medium with autoplay and recognition off;
`restored-status.json` retains the build/boot identity and state. UART/source
owners were released. Canonical verification uses `scripts/check`; its result
is recorded with the commit rather than inferred from these device trials.
