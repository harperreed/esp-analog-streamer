# Capture publication isolation

Work is in progress under the approved shared-ring design and implementation
plan. No hardware acceptance or dp08 completion is claimed yet.

## Target feasibility

The pinned ESP Rust distribution 1.98.1.0 reports rustc 1.98.1-nightly,
commit `183f762d61935f539d2d68d52d80bca4bf8b7c76`, LLVM 21.1.3. A throwaway
AtomicI32/AtomicU32 SeqCst probe was compiled for `xtensa-esp32-espidf` at both
optimization levels used here (3 for core, s for firmware).

Both generated direct 32-bit operations: loads use `l32i.n; memw`; stores use
`memw; s32i.n; memw`. The 480-sample loops retain those barriers. Object
inspection found no calls, CAS/S32C1I, 64-bit helpers or unresolved symbols.
LLVM IR retains seq_cst operations with four-byte alignment. This passes the
compiler feasibility check; final linked production paths still need inspection.

The standalone target lacks installed core libraries. The probe used cached
core/compiler_builtins libraries built by the same compiler, with explicit
`--extern` arguments. No firmware build or hardware change was needed.
Commands and full reports remain under the local SDD workspace; probe source,
IR, assembly and objects are retained under `target/capture-publication/`.

| Artifact | SHA-256 |
| --- | --- |
| atomic_probe.rs | e77a01b2aed20c79f12b048131bbcbf195439e5f6ffa0bc5ff8aa59155c4fb8e |
| atomic_probe.o (opt 3) | 78daeb59a3807a35b1c8770d4d962dcbf4b09ca8f26dddcf0103f40efe3f0bd3 |
| atomic_probe.o (opt s) | 593307069e48198d7b684ec4a2b656b2330f967a0f9a76269bd7c877b01466c2 |

Pinned IDF `components/xtensa/include/xtensa/cacheasm.h` documents MEMW's
ordering role. IDF's external-RAM atomic helper locks do not appear in these
objects. This checks the emitted code, not the compiler backend's complete
memory-model correctness. Runtime cost and cache-disabled PSRAM limitations
remain hardware concerns.

## Allocation review

Pinned Rust `alloc/src/sync.rs` defines ArcInner with repr(C), strong and weak
atomic counts followed by the data in one allocation. Pinned IDF
`heap_caps_base.c` chooses one registered heap for an allocation, and
`multi_heap_malloc_impl` allocates from that heap's TLSF pool. An Arc allocation
does not bridge internal RAM and PSRAM pools. Checking that the live ring object
is internal therefore also establishes the region for that allocation's counts,
without subtracting a guessed private header offset.

Firmware must check actual object/stamp storage endpoints as internal and
payload endpoints as PSRAM before capture starts. Normal allocation preference
alone is not the acceptance check. No custom allocator bridge is currently
justified. Real net memory/stack cost remains unmeasured for this change.
