# Source rebuild probe receipts

These are throwaway engineering probes and exact copies of measured receipts,
not release automation. See [the findings](../mp3-relink.md).
`manifest.json` records the source path and hash of each copied artifact.
Three raw artifacts use lossless gzip storage to preserve their whitespace;
the manifest records both compressed and original-byte hashes.
Generated SDK/linker files are retained once; both image receipts record their
identical hashes. Full symbol tables, proof ELFs and raw build logs stay in the
local target directories named in the receipts. No private media is included.

From the repository root, reproduce the source archive at the measured commit:

```sh
uv run --quiet --locked python3 docs/audit-evidence/2026-10-13/mp3-relink-probe/package.py \
  --revision bfea1d40adc401205ac67ec76d9ad411b9f78215 target/relink-source
uv run --quiet --locked python3 docs/audit-evidence/2026-10-13/mp3-relink-probe/verify.py \
  target/relink-source target/relink-extraction
```

Both output directories must be new. Verification needs the source commit in
local Git to compare committed blobs. `revision-option-validation.json` records
the packager's final hash; earlier `package-controls.json` predates that CLI
addition. The archive itself did not change.

For the measured target procedure:

1. Extract into an independent source tree. Create `firmware/cfg.toml` from
   the archived `public-cfg.toml` before any compilation. Keep the extraction's
   normal `firmware/target`; do not reuse production output.
2. Supply the pinned tools and dependencies. This run independently copied
   SDK/tool caches into extracted `firmware/.embuild/espressif`, excluding
   `python_env`, and let the SDK installer create its environment. External
   installed tools, Cargo caches and downloads were used. Record that scope.
3. Initialize a local Git snapshot of the manifest-listed public files, with
   normal hooks. The configuration remains ignored. Activate the verified mise
   configuration. Run `bash check-recipient.sh RECIPIENT OUTPUT_LOG`.
4. Inspect the complete canonical outcome. The measured bootstrap run failed
   its boundary check after SDK installation introduced dependency manifests.
   Its unchanged-source repeat passed. Do not hide this setup requirement or
   call the failed run a passing canonical check.
5. Use `record-target.py RECIPIENT CANONICAL_RUN NEW_OUTPUT` to retain the
   passing baseline. Decompress `private-bit.patch.gz` and apply the patch
   inside the extraction, rerun the
   same canonical scope, and retain modified output separately.
6. Compile `offset.c` with the copied Xtensa GCC, the extracted Shine include
   directory, `-mlongcalls -S`; compare the actual final initializer/bridge
   disassemblies. `target-verification.json` records the observed instructions.

The build wrapper uses a small environment allowlist and disables interactive
Git prompts/global URL rewrites only within the check process. It does not
change global settings. The canonical script still sources the installed
`~/export-esp.sh`; here that file supplies LIBCLANG_PATH only. This is a declared
machine dependency, not a hermetic environment. No flashing command is used.

For the actual host comparison, copy `host/run.py` and `host/probe.c` together
into a fresh directory beneath this Git repository, then run the copied script
with uv. It freezes only the MP3 subtree at the measured revision, compiles the
real C bridge/library twice, checks the bit change and decodes both outputs.
It requires `cc`, FFmpeg and Git. It generates synthetic audio without opening
an audio device. Existing output directories cause an error; keep prior runs.

The modified golden bytes are intentionally different. Neither the host
probe nor `--firmware` claims a full unmodified host-suite pass. This evidence
does not grant distribution approval, prove an external recipient's build,
or establish runtime/streaming reliability.
