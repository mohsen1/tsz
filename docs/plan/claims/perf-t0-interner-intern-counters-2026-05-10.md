# perf(solver,common): wire interner intern_calls/hits/misses

- **Date**: 2026-05-10
- **Branch**: `perf/t0-interner-intern-counters-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Performance) — Tier 0.3 follow-up

## Intent

Wire the three top-level interner counters (`intern_calls`, `intern_hits`,
`intern_misses`) at the actual `TypeInterner::intern` entry/exit sites in
`crates/tsz-solver/src/intern/core/interner.rs`. These fields are declared
in `crates/tsz-common/src/perf_counters.rs` but never written, so the
T0.3 perf-counter JSON has reported them as `null` since #4948.

This unblocks one half of the Tier 2.4 (interner redesign) decision per
`docs/plan/PERFORMANCE_PLAN.md` §2 status table: with these counters
plus the existing `*_intern_calls` per-kind buckets we can tell whether
the 7M+ string interns observed on monorepo-006 are dominated by hits
(volume issue) or misses (allocation issue). The `lock_wait_histogram_ns`
(contention) signal is intentionally out of scope here; it requires a
new `perf-counters-timing` cfg feature gate per §4.T0.3.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs` — count `intern()`
  calls; credit hits/misses at every disposition (intrinsic short-circuit,
  TL-cache hit, shard `key_to_index` hit, race-loss `Entry::Occupied`,
  `Entry::Vacant` insert).
- `crates/tsz-common/src/perf_counters.rs` — flip
  `wired.interner_intern_calls = true`, populate `Some(...)` for the three
  intern counters in `snapshot()`, switch the `dump_string` `n/a (not
  wired in this PR)` placeholders to numeric output, and update the
  `unwired_buckets_serialize_as_null` test plus add a positive
  `wired_intern_call_buckets_serialize_as_numbers` test.

## Verification

- `cargo nextest run -p tsz-common -E 'test(json_tests)'` — 6/6 pass
  (includes the new `wired_intern_call_buckets_serialize_as_numbers`).
- `cargo nextest run -p tsz-common -p tsz-solver --lib` — 6145/6145 pass.
- `cargo clippy -p tsz-common -p tsz-solver -p tsz-cli --features
  tsz-cli/perf-tools --all-targets -- -D warnings` — clean.
- End-to-end: `TSZ_PERF_COUNTERS=1 tsz --project tsconfig.json
  --perf-counters-json /tmp/pc.json` on a 5-line generic-heavy fixture:
  `intern_calls = 3628`, `intern_hits = 2857`, `intern_misses = 771`
  (hits + misses = calls exactly, confirming the wiring covers every
  disposition). `wired.interner_intern_calls = true`.
- Timing-mode (counters disabled): same fixture reports `enabled: false`,
  `mode: "timing"`, all three counters `0`. Cheap-disabled path preserved.

## Out of scope (explicit follow-ups)

- `interner.lock_wait_histogram_ns` — requires a new `perf-counters-timing`
  cfg feature gate per `PERFORMANCE_PLAN.md` §4.T0.3 lock-wait shape.
- `resolver.{is_file,is_dir,read_dir}_calls` — separate next PR per the
  same T0.3 follow-up list (2026-05-10 scale-cliff summary).
- `PhaseTimings` config_discovery/source_discovery/module_resolution/
  load_libs split — separate next PR per T0.2 follow-up list.
- Re-running monorepo-006 cliff in attribution mode and updating the
  decision record (`docs/plan/perf-runs/2026-05-10-scale-cliff-summary.md`)
  with a hits/misses breakdown — done after this PR lands.
