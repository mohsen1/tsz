# perf(solver): wire interner_string_intern_calls counter

- **Date**: 2026-05-09
- **Branch**: `perf/wire-interner-string-intern-calls`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T1.1 (instrumentation — single-site counter wiring)

## Intent

The `interner_string_intern_calls` counter exists in
`crates/tsz-common/src/perf_counters.rs:381` but neither side is
wired:

- The hot caller `intern_string` at
  `crates/tsz-solver/src/intern/core/interner.rs:817` does not
  increment it.
- `dump_string()` at
  `crates/tsz-common/src/perf_counters.rs:644` prints
  `n/a (not wired in this PR)` for this row.

Wire both. This is the smallest unit of T1.1 (site #2 from
PERFORMANCE_PLAN.md §4.1.1) and unblocks `--perf-counters` users
who want to attribute string-interner traffic without taking on the
full ten-counter wire-up at once.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs` (+3 LOC)
- `crates/tsz-common/src/perf_counters.rs` (-1/+2 LOC: replace `n/a`
  template line + insert `load(&c.interner_string_intern_calls)`)

## Verification

- `cargo check -p tsz-common -p tsz-solver` (clean)
- `cargo nextest run -p tsz-common -p tsz-solver --lib` (6124 passed,
  7 skipped)
- Disabled-path overhead: `inc()` is `#[inline(always)]` and
  branch-tests `enabled_fast()` first; verified no extra branches in
  the timing-mode hot path.
