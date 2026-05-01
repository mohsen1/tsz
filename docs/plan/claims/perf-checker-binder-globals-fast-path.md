# perf(core,cli): fast-path globals merge into file_locals

- **Date**: 2026-05-02
- **Branch**: `perf/checker-binder-globals-fast-path`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (large-repo residency)

## Intent

The post-merge per-file checker binder reconstruction folds
`program.globals` into each file's `file_locals` so checker lookups
(`Promise`, `Iterable`, `React`, …) resolve through `binder
.file_locals.get(name)`. Three call sites (CLI driver, parallel core
legacy, parallel core shared) implement the same merge by allocating a
fresh `FxHashMap` and inserting both sides — O(L + G) `String` clones
per binder reconstruction. With ~6086 files in `large-ts-repo` and
~5 K lib globals, that's ≥30 M `String` clones across the multi-file
pipeline.

This change consolidates the merge into a `MergedProgram::
build_merged_file_locals` helper and adds two fast paths that exploit
`SymbolTable` being internally `Arc<FxHashMap>` (since #1535):

- `local_count == 0` → `Arc::clone(globals)` (O(1)). Common for
  trivial declaration files / pure re-export modules.
- `globals.is_empty()` → `Arc::clone(file_locals[file_idx])` (O(1)).
  Hits in LSP probes / minimal harness setups.

The slow path keeps the pre-sized fresh-`FxHashMap` insert pattern and
preserves the previous "locals win on collision" semantics. All three
call sites delegate to the helper.

## Files Touched

- `crates/tsz-core/src/parallel/core.rs` (helper + 2 call site swaps)
- `crates/tsz-cli/src/driver/check_utils.rs` (1 call site swap)
- `crates/tsz-core/tests/parallel_tests.rs` (3 new unit tests)

## Verification

- `cargo nextest run -p tsz-core` — all green
- `cargo nextest run -p tsz-cli` — same passing set as `main`
- New unit tests cover: combined locals + globals merge, locals-win
  collision, empty-locals fast path.
