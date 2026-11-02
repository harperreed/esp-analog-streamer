# Pinned Shine integration and distribution review

The e8hd source review is complete; distribution acceptance is not. The
integration has a verifiable upstream pin and documented local changes. Six
modified files lacked dated local notices; this change supplies them without
changing the code or license. A releasable source/relinking package has not
been demonstrated. Keep that requirement open before distributing firmware.

## Provenance and integration

At repository revision `0320627`, all 23 entries in
`mp3/vendor/shine/upstream-sha256.json` match the original files at
[`ab5e3526b64af1a2eaa43aa6f441a7312e013519`](https://github.com/toots/shine/tree/ab5e3526b64af1a2eaa43aa6f441a7312e013519).
This comparison used the retained upstream Git object, not mutable upstream
HEAD. Only the six files documented in UPSTREAM.md differ: bitstream.c/.h,
l3loop.c, layer3.c/.h and types.h. The retained COPYING matches upstream.
[shine-review.json](shine-review.json) records original and reviewed hashes.

`mp3/build.rs` compiles nine Shine C units and the local bridge into a static
archive; Cargo carries them in the MP3 rlib linked into firmware. It uses
`-O3 -fsigned-char`, and the device build selects the classic ESP32 compiler
and long-call convention. `types.h` selects generic multiplication on Xtensa;
the retained ARM/MIPS paths are not evidence of supported firmware targets.
`firmware/build_support/mp3_iram.rs` places these ten members' instructions and
literals in IRAM. An encoder update must preserve that measured placement or
repeat its existing device acceptance; a host benchmark alone is insufficient.

The wrapper owns one encoder, accepts only 48 kHz stereo 320 kbps, and validates
960-byte frames and zero bit-reservoir dependency before exposing bytes. Local
patches provide checked per-instance allocation/deallocation, fixed output
storage and sticky bounds failure. Existing real-component tests exercise
both allocation failures, cleanup, separate instances, zero-bit writes,
bounds guards, a pinned independent-noise output and actual FFmpeg decoding.
These are scoped regression protections, not proof against every C defect.
No tail-drain API is supplied. Previously measured gain, warmup and finite-tail
limits remain; the upstream README also states there is no psychoacoustic model.

Maintenance rule: retain the exact upstream object and original hashes,
compare the six local patches, then rerun canonical checks and the existing
allocation, decoder, join, instruction-placement and loaded-device gates for
any update. The manifest hashes original upstream bytes, not patched bytes.
Do not replace its entries with local hashes. No latest-release, vulnerability-
free or ongoing upstream-support claim was established by this pinned review.

## License facts and unresolved release work

The retained [COPYING](../../../mp3/vendor/shine/COPYING) is the GNU Library
General Public License version 2. The source headers contain no separate
version grant. The pinned JS package says LGPL-2.0-only, but its metadata is
not a replacement grant for these C files. Do not infer an applied “or later”
grant from the example notice at the end of COPYING. Project Rust/bridge
GPL-3.0-or-later metadata does not itself relicense Shine.

COPYING section 2(b) calls for dated modification notices. Git commit
`e66f885d4b9a7fe7a02004e56390ef5c853b0314` dates the six local code changes to
October 9, 2026; the new comments record that fact and their October 13 addition.
Original author notes remain. Section 3 describes an optional conversion;
this change does not exercise it.

For the retained license, sections 4 and 6 describe source, notice and relinking
conditions for binary distribution. The recommended engineering path is to
prepare corresponding source and the required build/relink materials beside
the binary, then verify a recipient can rebuild with a modified library.
No release package or recipient rebuild has been verified here. Distribution
terms and compatibility of the complete firmware remain a release decision;
this component review is not legal clearance for the whole image.

The repository contains build scripts, locks, config examples and both license
texts, but its inspected CI uploads check logs rather than such a binary/source
package. Do not ship private credentials as build configuration. Supplying a
repository URL or retaining COPYING alone does not prove packaging obligations
have been met. No license conversion, publication or dependency upgrade occurs
in this change. Primary text: [GNU Library GPL v2](https://www.gnu.org/licenses/old-licenses/lgpl-2.0.html)
and the exact pinned COPYING above. The independent reviewer retrieved the
exact pinned upstream files through the web, including [README.old](https://raw.githubusercontent.com/toots/shine/ab5e3526b64af1a2eaa43aa6f441a7312e013519/README.old)
and [package metadata](https://raw.githubusercontent.com/toots/shine/ab5e3526b64af1a2eaa43aa6f441a7312e013519/js/package.json).
GNU-site retrieval timed out; license terms were checked against the
byte-verified pinned COPYING.

## Verification and scope

A clean-context Sol 6.1 agent independently inventoried grants, notices and
provenance. The maintenance agent hit capacity before completing; the parent
performed that source review directly. The modification is documentation and
C comments only; no test of comment wording substitutes for code verification.
Canonical results and a token-preserving source comparison are recorded in the
JSON. No board reset, playback, settings, authentication or network change was
needed. e8hd remains open for recovery, endurance, fidelity and distribution
acceptance. The completed preset-offset gate remains complete.

All 27 canonical stages passed before the two notice-wording refinements;
all five firmware stages passed afterward. Final host preprocessing is
byte-identical across all nine C units, and the rebuilt ordinary firmware
remains byte-identical to `1d2105b`. No device test was needed for these
non-executable changes. Parent fresh-eyes review found no remaining blocker
for this scoped change; distribution acceptance remains open.
