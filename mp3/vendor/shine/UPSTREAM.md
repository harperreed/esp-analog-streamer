# Pinned Shine source

Source: https://github.com/toots/shine
Commit: ab5e3526b64af1a2eaa43aa6f441a7312e013519
Version reported by the benchmark: 3.1.1-59-gab5e352

This directory vendors the nine encoder compilation units, their library headers,
and COPYING (GNU Library General Public License version 2). All original source
notices remain. `upstream-sha256.json` records the **unmodified upstream** files;
it is a provenance record, not a digest of the locally patched files.

Local patches:

- `layer3.h`, `types.h`, `layer3.c`: initialization takes allocation/deallocation
  callbacks and a context. Each state stores its own deallocator and context.
  Both initial allocations are checked; partial construction releases state.
  State remains zero initialized. Close releases both owned buffers through the
  same allocator. The internal C initialization signature is intentionally changed.
- `bitstream.h`, `bitstream.c`: initialization accepts already allocated storage.
  Output never reallocates; bounds failure becomes sticky and makes encoding
  return failure. Validate bit widths before shifting or writing; zero-width writes
  return immediately to avoid a shift by 32. A regression test reproduced the
  upstream zero-width cache corruption before this correction. The bitstream
  retains the upstream 4,096-byte capacity.
- `layer3.c`: propagate sticky bitstream failure as null output with negative size.
- `bitstream.c`, `l3loop.c`: make signedness comparisons explicit and mark unused
  parameters to keep the normal C build warning-free without suppressing warnings.
- `l3mdct.c`: normalize the long-block transform coefficients by 8/9 during
  initialization. Analysis and MDCT fixed-point products otherwise contribute
  1/8, while inverse overlap contributes 9. See the
  [normalization evidence](../../../docs/audit-evidence/2026-10-13/mp3-normalization.md).
- Patched files use LF line endings.

The Rust API permits only 48 kHz stereo 320 kbps. Its bridge checks each frame's
format and zero reservoir dependency before copying 960 output bytes. No upstream
command-line tools, JavaScript bindings or build system are included.

The Rust wrapper and bridge follow this project's GPL-3.0-or-later license, as
recorded in Cargo.toml. The vendored Shine component retains its own license
in COPYING; the crate metadata does not replace that component license.

The [integration and license review](../../../docs/audit-evidence/2026-10-13/shine-review.md)
records the exact pin, dated local modifications and unresolved distribution
requirements. Local change notices identify the October 9 and 13 code changes; they
do not change Shine's license. No license conversion has been selected.

This integration is a prototype. Distribution/relinking acceptance remains
unproven. Shine has no psychoacoustic model. Encoder delay,
finite-stream tail handling and noise fidelity remain adoption limitations; see
`docs/audit-evidence/2026-10-09/mp3-benchmark.md` in the repository.

## Pinned output comparison

A 16-frame independent-noise fixture uses xorshift32 seed `0x12345678`, with
operations `state ^= state << 13; state ^= state >> 17; state ^= state << 5`
on wrapping unsigned 32-bit state. Each interleaved signed sample takes the
state's high 16 bits. The input has 36,864 samples and SHA256
`c6f048ed066614b109d9b2494488394854bd448c215407c697af98519a9956f0`.
The unmodified pinned sources, compiled with `-O3 -fsigned-char` and the archived
`docs/audit-evidence/2026-10-09/mp3-probe/host/probe.c`, produced 15,360 bytes:
SHA256 `ebff59601676c16355dd9d63801e230be90293c767f19816737e1b5101b9cb6b`;
FNV-1a-64 `ccb10bf832c51705`. This historical unnormalized output is retained
as provenance, not the current expected output.

The October 13 independent build applies only MDCT coefficient normalization
to that pinned source. It produces SHA256
`9e76b1a9ad1d4c703d0512ce815e1eae37f31a73afc5ccf41e1214084b3350ec`;
FNV-1a-64 `ece25e7178a4d8df`. The Rust integration test recreates the fixture
and checks that complete output. Production also matches the normalized
upstream build byte-for-byte on the retained tones and noise fixtures.
These comparisons do not prove identical output for every PCM input.
