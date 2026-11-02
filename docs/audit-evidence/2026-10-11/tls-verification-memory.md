# TLS verification allocation failure

The October 11 diagnostic reproduced a memory allocation failure inside RSA
certificate verification during real Sonos setup with competing debug listeners.
Mbed TLS converted that error into NOT_TRUSTED. Kata 7vhq remains open: neither
this diagnostic nor the subsequent allocator-threshold trial reached a Sonos
listener join. No recovery success is claimed.

## Evidence

The workstation verified the current coordinator's chain using the exact
firmware Sonos root file. This host result does not prove the board has enough
memory to verify it. The effective ESP32 configuration disables Mbed TLS date
checks, so adding a clock gate would not address this observed failure.

Two temporary, failure-only ROM print calls exposed the swallowed return code:

```text
X509_DIAG signature_ret=-17040 top=1
TLS_DIAG ret=-9984 flags=8 free=7511 largest=7424
```

The pinned Mbed TLS headers define -0x4280 as RSA_PUBLIC_FAILED and -0x0010 as
MPI_ALLOC_FAILED. Their sum is -0x4290 (-17040). The RSA cleanup path combines
these errors; X.509 parent selection converts a failed signature check into
NOT_TRUSTED (8), eventually returning -0x2700. The raw allocation size is unknown.
The 7511/7424-byte heap values were recorded after signature-check cleanup,
not at the failing allocation.

Diagnostic image:
`968aa68eb005886bbc2b2c96a5e47c21fafa253f274d8fc1a425107bbe66ac72`.
The failed trial verified that image and one boot before/after. Driver cleanup
restored stopped MP3/Medium with no cleanup errors. The two patched SDK sources
were restored and checked against their original SHA-256 hashes; subsequent
normal firmware contains neither diagnostic string.

## One-setting trial: rejected

The candidate changed only the general allocation threshold from 8191 to 2047.
The 3840-byte FLAC partition-sums vector then prefers PSRAM, while the 1920-byte
capture buffers retain internal preference. TLS still explicitly requested
internal RAM. Actual allocation addresses were not measured, so placement is not
claimed as observed.

All 27 canonical stages passed in run.0VkX36b9, with matching start/end tracked
boundaries. Candidate image:
`ec3ac7aca5266c98fb9984a4a1b749761d6006fed710f90a1148fbcfb22bf70d`.
Its effective configuration retained internal TLS allocation and the 32768-byte
internal reserve. The normal flash build matched the archived image.

Before-connect internal free memory was 35163 bytes, compared with 31531 in the
diagnostic trial (3632 bytes more); largest free block was 32768 versus 24576.
These separate trials do not isolate every allocation. TLS still returned
-0x2700, and no Sonos listener joined. The uninstrumented candidate does not
prove the exact inner error was identical. Its FLAC encoder reported 118 frames
in 10084842 us, zero raw overruns/resets, and a 19699-us encode maximum for that
window. This is not sample-exact playback acceptance.

The candidate verdict remains FAILED, cleanup passed, and the threshold change
was reverted. Reconnect/cancel/failed-start and MP3 timing cases were not run
because the first required contention gate failed.

## Driver correction

The prior failure cleared the native queue before the driver's ownership read,
which then returned HTTP 404 and prevented Stop. The scratch driver now allows
only Error/no-active-player plus exact original media/config, fresh coordinator
and matching image/boot to invoke the board's session-scoped Stop. All other
paths retain the existing ownership check. It never sends a general SOAP Stop.
The recorded-failure regression failed before the change; twelve scratch tests
and both subsequent actual failed-trial cleanups pass. Old failure receipts were
not overwritten or reclassified.

## Normal restoration

The original 8191 threshold and internal TLS allocator were restored. Canonical
firmware run.R4rluLpM passed all five stages. Image
`d448497eb0892bc25287c95a4cc0d7a5ab6a1ca42baba57b6627cefc4a75d540`
was flashed and attested on boot `c782967a7c46e501639c28cd385f399a`: stopped
MP3/Medium, autoplay/recognition off, no active player. Source/UART processes
were stopped. This restoration is separate from the two failed trial receipts.

## Pending decision

A reviewed, explicit proposal would restore general allocation threshold 8191
and change only this allocator choice in `firmware/sdkconfig.defaults`, plus its
explanatory comment:

```diff
-CONFIG_MBEDTLS_INTERNAL_MEM_ALLOC=y
+# CONFIG_MBEDTLS_INTERNAL_MEM_ALLOC is not set
+CONFIG_MBEDTLS_EXTERNAL_MEM_ALLOC=y
```

This supported external allocator would affect all Mbed
TLS users, including recognition HTTPS and secure MQTT when enabled. TLS secrets
and plaintext could reside in unencrypted external RAM on this ESP32. Roots,
verification, record capacities, task stacks and listener limits would stay fixed.
The project requires approval for security-related changes; that question is
pending. No external-TLS candidate has been built or flashed.

Raw receipts, exact images, source patches/hashes, frozen driver and proposal are
under `target/kata-hardening/tls-verification/`. The companion JSON retains the
important values and artifact hashes. These results explain the setup failure;
they do not satisfy 7vhq or reduce the open count of 18.
