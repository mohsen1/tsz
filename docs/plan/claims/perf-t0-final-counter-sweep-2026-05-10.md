# perf(checker,cli): finish gate-once-cached counter sweep

**2026-05-10 15:05:00**

## Scope

Final two `inc(&counters().X)` sites that still pay an unconditional
`OnceLock<PerfCounters>::get()` per call:

1. `crates/tsz-checker/src/state/state.rs::record_root_checker_construction`
   — bumps `checker_state_constructed` once per checker construction
   (per-file, ~1.28× files in attribution mode per
   `docs/plan/perf-runs/2026-05-10-scale-cliff-summary.md`).

2. `crates/tsz-cli/src/driver/sources.rs` — bumps
   `resolver_lookup_calls` once per module specifier resolution
   (O(imports per file)).

These are mid-frequency paths (not millions/run like the previous
sweeps), but they're the last inline sites in the codebase. Bringing
them onto the same shape closes the consistency story so future
counter additions have one obvious template to copy.

## Approach

Same `if enabled_fast() { inc(&counters().X); }` pattern as the
previous sweeps (#4966, #4960, #5000, #5004, #5009).

## Behavior

- **Enabled mode** (`TSZ_PERF_COUNTERS=1`): counter values unchanged.
- **Disabled mode** (default): each call site drops one
  `OnceLock<PerfCounters>::get()` per invocation.

No semantic change.

## Verification

- `cargo check -p tsz-checker -p tsz-cli` clean
- Pre-commit hook (fmt, clippy, arch guard, full nextest suite) — to
  be confirmed before push

## Conformance

Counter values, diagnostics, and conformance snapshots unaffected.
