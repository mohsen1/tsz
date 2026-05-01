# perf(cli): Arc-clone cross-file lookup binder file_locals

- **Date**: 2026-05-01
- **Branch**: `perf/cross-file-binder-arc-clone-locals`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (large-repo residency)

## Intent

`create_cross_file_lookup_binder_with_augmentations` rebuilds the per-file
`SymbolTable` by allocating a fresh `FxHashMap` and inserting every
`(name.clone(), sym_id)` entry one by one. Since PR #1535 made
`SymbolTable` internally `Arc<FxHashMap<String, SymbolId>>`, a plain
`.clone()` on the source table is an O(1) atomic refcount bump. This
change replaces the manual rebuild with `program.file_locals[file_idx]
.clone()`, eliminating one fresh `FxHashMap` allocation and N `String`
clones per cross-file lookup binder. With ~6086 files in
`large-ts-repo` and the cross-file binder build path running once per
file under rayon, this removes a measurable allocator-pressure source
during the multi-file pipeline's startup.

The per-file checking binder (`create_binder_from_bound_file_with_
augmentations`) is left unchanged in this PR because it has to merge
`globals` into `file_locals`, which still requires `Arc::make_mut`
plus per-entry inserts. That path is a separate follow-up.

## Files Touched

- `crates/tsz-cli/src/driver/check_utils.rs` (~10 LOC change)

## Verification

- `cargo nextest run -p tsz-cli`
- `cargo nextest run -p tsz-binder`
