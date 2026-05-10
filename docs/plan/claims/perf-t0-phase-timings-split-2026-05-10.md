# perf(cli): split PhaseTimings sub-phase buckets

- **Date**: 2026-05-10
- **Branch**: `perf/t0-phase-timings-split-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Performance) — Tier 0.2 follow-up

## Intent

Wire `PhaseTimings::config_discovery_ms`, `source_discovery_ms`, and
`module_resolution_ms` as first-class struct fields so the diagnostics
JSON schema's reserved keys are no longer hardcoded `0.0` in
`perf_json::build_report`. This is the structural prerequisite for a
future PR that wraps `Instant::now()` around the actual
config/source-discovery/module-resolution call sites and populates the
buckets.

Per the 2026-05-10 scale-cliff summary "Follow-up gaps" #1, the JSON
phase-split currently rolls config/source/module-resolution into
`io_read` and `parse_bind`. With this PR the struct can carry the
attribution; the next PR can attribute it.

## Approach

Pure infrastructure / schema-faithful split:

1. Add three `f64` fields to `PhaseTimings` (defaults to `0.0` via the
   existing `derive(Default)`).
2. Update the explicit `PhaseTimings { ... }` construction site in
   `core.rs::compile()` to use `..PhaseTimings::default()` so it picks
   up the new fields without needing to set them yet.
3. In `perf_json::build_report`, replace the three hardcoded `0.0`s
   in `PhasesMs.config_discovery / source_discovery /
   module_resolution` with reads from the new struct fields.
4. Extend the existing `phase_timings_*` driver test to assert the
   three sub-phase fields are non-negative (the only invariant they
   must satisfy until the driver actually attributes time to them).

Sub-phase buckets are subsets of the existing top-level buckets they
came out of (config/source/module-resolution land inside `io_read`;
the future driver attribution will *move* time up rather than create
new wall time), so the existing `total_ms >= sum-of-phases`
sanity-check is unchanged.

## Files Touched

- `crates/tsz-cli/src/driver/core.rs` — add 3 `f64` fields to
  `PhaseTimings`, update the explicit construction site to use
  `..PhaseTimings::default()`.
- `crates/tsz-cli/src/perf_json.rs` — read the three new fields in
  `build_report` instead of hardcoded zeros.
- `crates/tsz-cli/tests/driver_tests.rs` — extend the timing
  invariant test with non-negative assertions for the three sub-phase
  buckets.

## Verification

- End-to-end on a small fixture: diagnostics JSON now reports
  `config_discovery=0.0, source_discovery=0.0, module_resolution=0.0,
  io_read=0.47, load_libs=114.87, ...` — the legacy `io_read` still
  carries the unattributed time, the schema now exposes a place for
  attribution to land.
- `cargo build -p tsz-cli --bin tsz --features perf-tools` clean.
- `cargo nextest run -p tsz-common -E 'test(json_tests)'` — 6/6 pass.
- `cargo clippy -p tsz-cli -p tsz-common --features tsz-cli/perf-tools
  --all-targets -- -D warnings` clean.

## Out of scope (next PR)

- The actual attribution of wall-time to the new buckets. That requires
  wrapping `Instant::now()` around config-discovery / source-discovery
  / module-resolution call sites and decrementing `io_read_ms` /
  `parse_bind_ms` by the same amount so `total_ms` stays consistent.

## No conformance / behavior impact

Pure schema-shape change. Driver behavior is identical; the JSON
output gets three extra struct-driven (vs. hardcoded) zero-valued
fields, and existing fields are unchanged.
