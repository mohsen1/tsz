# perf(core): pre-size cross_file_node_symbols at merge time

- **Date**: 2026-05-02
- **Branch**: `perf/presize-cross-file-node-symbols`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (large-repo residency)

## Intent

`merge_bind_results_from_source` builds `cross_file_node_symbols`
(`FxHashMap<usize, Arc<FxHashMap<u32, SymbolId>>>`) one entry per file
in a tight loop:

```rust
for file in &files {
    let arena_ptr = Arc::as_ptr(&file.arena) as usize;
    cross_file_node_symbols.insert(arena_ptr, Arc::clone(&file.node_symbols));
}
```

The map starts at zero capacity (`FxHashMap::default()`) and grows
through the default doubling schedule (1 → 3 → 7 → 15 → 31 → … →
8191 buckets for ~6086 files). On a 6086-file project that's roughly
13 rehashes during the merge phase, each one allocating a fresh
bucket array and re-inserting every prior entry.

The exact final size is known up-front — `results.len()` is the file
count and equals the eventual entry count — so this change pre-sizes
the map at `with_capacity_and_hasher(results.len(), Default::default())`,
skipping the rehash chain. No behavior change.

## Files Touched

- `crates/tsz-core/src/parallel/core.rs` (~6 LOC)

## Verification

- `cargo check -p tsz-core` — clean
- `cargo nextest run -p tsz-core -E 'test(test_merge)'` — 13/13
