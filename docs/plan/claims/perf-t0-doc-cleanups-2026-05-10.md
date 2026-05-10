# docs(common,checker): tighten T0.3 follow-up doc imprecisions

- **Date**: 2026-05-10
- **Branch**: `perf/t0-doc-cleanups-2026-05-10`
- **PR**: #4996
- **Status**: ready
- **Workstream**: 4.T0.3 (perf counter follow-up documentation)

## Intent

Three docs-only fixes addressing unaddressed Copilot review comments on
already-merged T0.3 follow-up PRs (#4984, #4987). All three flag the
same shape of issue: the doc comment slightly overstates what compiles
out / what is "visible".

1. `record_lock_wait_ns` doc (PR #4987): the function and its caller
   `time_shard_write` are described as compiling out entirely when
   `perf-counters-timing` is off. In reality the function item itself
   doesn't exist (the `cfg` excludes it) but `time_shard_write` *does*
   still exist as a feature-off no-op stub that calls `f()` directly.
   The new wording says exactly that.

2. `lock_wait_histogram_wired` doc (PR #4987): says feature-off builds
   have "no histogram code at all", but the histogram field
   (`interner_lock_wait_histogram_ns: [AtomicU64; 8]`) is unconditional
   and remains in the `PerfCounters` struct (feature-stable layout).
   What's compiled out is the timing+recording logic. New wording
   distinguishes "fields stay, timing logic compiles out, snapshot
   serializes as `null`".

3. `SymbolFileTargetsNode::total_entries` doc (PR #4984): says "Total
   entries visible through this node" but the implementation reports the
   represented snapshot size. Layered snapshots can count shadowed keys
   once per layer, while flattened snapshots merge layers and count each
   visible key once. New wording distinguishes both paths.

## Files Touched

- `crates/tsz-common/src/perf_counters.rs` (doc comments only)
- `crates/tsz-checker/src/context/symbol_file_targets.rs` (doc comment only)
- `docs/plan/claims/perf-t0-doc-cleanups-2026-05-10.md` (claim metadata)

## Verification

- `cargo fmt --all --check`
- `cargo clippy --profile ci-lint -p tsz-common -p tsz-checker --all-targets -- -D warnings`
