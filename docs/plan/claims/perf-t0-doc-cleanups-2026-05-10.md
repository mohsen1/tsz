# perf(common,checker): clean up T0.3 follow-up doc imprecisions

**2026-05-10 13:30:00**

## Scope

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
   entries visible through this node" but the implementation is
   `parent_total + own_entries.len()`, which over-counts when a delta
   entry shadows a parent key. New wording explicitly calls out
   multi-set semantics and explains the counter use case (sizing the
   parent chain) where multi-set matches the cost model better than
   de-duplicated visibility.

## Behavior

No runtime change. All edits are inside doc comments.

## Verification

- `cargo check -p tsz-common -p tsz-checker` clean
- Pre-commit hooks (fmt, clippy, arch guard, full test suite) pass
- No new clippy warnings (doc-comment text wraps within `clippy::doc_markdown`
  conventions — backtick-quote identifiers like `PerfCounters`,
  `time_shard_write`, `enabled_fast`, etc.)

## Conformance

Docs-only. Snapshots unchanged.
