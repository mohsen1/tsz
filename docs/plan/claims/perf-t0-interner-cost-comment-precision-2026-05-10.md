# perf(solver): tighten interner cost-summary comment for accuracy

- **Date**: 2026-05-10
- **Branch**: `perf/t0-interner-cost-comment-precision-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Performance) — docs follow-up to merged #4960

## Intent

Address two Copilot review comments left on #4960 (gate-once
interner counter cleanup) that the summary oversimplified the cost
model. Pure docs change in `crates/tsz-solver/src/intern/core/interner.rs`.

## What Copilot flagged

The previous comment block on `intern()` said:

```text
We gate once with `enabled_fast()` and cache the counter pointer
so an enabled run pays one `OnceLock` deref per `intern()` call
instead of one per increment, and a disabled run pays only the
single fast-gate load (each increment compiles to a no-op).
```

Two precision issues:

1. "One `OnceLock` deref" understates: enabled mode actually performs
   *two* `OnceLock` reads — the `enabled_fast()` gate is itself an
   `OnceLock<bool>` read, and `counters()` is a separate
   `OnceLock<PerfCounters>` deref.
2. "Each increment compiles to a no-op" is loose: each
   `if let Some(c) = pc { … }` is a runtime `Option` match against a
   local. The optimizer makes it a predictable, often-folded branch,
   not literally a `nop`.

## What this PR changes

Replace the comment with a precise version:

```text
We gate once with `enabled_fast()` (one `OnceLock<bool>` read) and
cache the resulting `&'static PerfCounters` pointer in `pc`. An
enabled run pays the gate read plus one `counters()`
`OnceLock<PerfCounters>` deref per `intern()` call (vs. one per
increment). A disabled run pays only the gate read: subsequent
`if let Some(c) = pc` checks are predictable branches on a local
`None`, so the increment body is consistently skipped.
```

No code changes. The cost story is unchanged; the comment is just
more truthful about the mechanism.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs` (4 comment lines updated)

## Verification

- `cargo check -p tsz-solver` clean.
- No behavior change.

## No conformance / behavior impact

Pure documentation. Counter values, gate semantics, and call paths
are identical.
