# perf(cli,common): wire resolver fs probe counters (is_file/is_dir/read_dir)

- **Date**: 2026-05-10
- **Branch**: `perf/t0-resolver-fs-counters-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Performance) — Tier 0.3 follow-up

## Intent

Wire the three remaining resolver fs probe counters
(`resolver.is_file_calls`, `is_dir_calls`, `read_dir_calls`) per the
2026-05-10 scale-cliff summary follow-up gap #3. With this PR plus the
already-merged interner counter wiring (#4955, #4960), all of the
counter buckets surfaced in the T0.3 schema are wired except
`interner.lock_wait_histogram_ns` (which the plan explicitly defers
to a separate `perf-counters-timing` cfg gate).

The fs counters confirm the 2026-05-10 attribution decision matrix
finding that resolver lookups are not on the cliff hot path.

## Approach

Rather than sprinkle inline `tsz_common::perf_counters::inc(...)` calls
before every `Path::is_file()` site (21 in `resolution.rs`), introduce
three thin counting wrappers near the top of the file:

```rust
fn count_is_file(path: &Path) -> bool { inc(...); path.is_file() }
fn count_is_dir(path: &Path) -> bool { inc(...); path.is_dir() }
fn count_read_dir(path: &Path) -> std::io::Result<std::fs::ReadDir> {
    inc(...); std::fs::read_dir(path)
}
```

All resolver call sites in `crates/tsz-cli/src/driver/resolution.rs`
now route through these wrappers. The diff is one token per call site,
the wrappers themselves are zero-cost when `TSZ_PERF_COUNTERS` is
unset (the `inc()` helper short-circuits on `enabled_fast()`), and
future migration to a real `CountingFs` trait (per
`PERFORMANCE_PLAN.md` §5) becomes a one-place change in this file
rather than 21.

## Files Touched

- `crates/tsz-cli/src/driver/resolution.rs` — add 3 wrapper helpers,
  sweep 21 call sites.
- `crates/tsz-common/src/perf_counters.rs` — flip
  `wired.resolver_fs_probes = true`, populate `Some(...)` for the
  three resolver counters in `snapshot()`, replace three
  `n/a (not wired in this PR)` placeholders in `dump_string` with
  numeric output, update the `unwired_buckets_serialize_as_null`
  test, add positive `wired_resolver_fs_probe_buckets_serialize_as_numbers`
  test.

## Verification

- End-to-end attribution mode on a small fixture:
  `is_file=1, is_dir=1, read_dir=0, package_json_reads=1, lookup=0`.
  All wired flags confirmed `true` in the snapshot's `wired` map.
- Timing mode (`TSZ_PERF_COUNTERS` unset): all three counters return
  zero (`enabled_fast()` short-circuits). No perf cost.
- `cargo nextest run -p tsz-common -E 'test(json_tests)'` — 7/7 pass.
- `cargo clippy -p tsz-cli -p tsz-common --features tsz-cli/perf-tools
  --all-targets -- -D warnings` clean.

## Out of scope (separate follow-ups)

- `interner.lock_wait_histogram_ns` — needs the `perf-counters-timing`
  cfg feature gate per the plan §4.T0.3.
- `PhaseTimings` config_discovery/source_discovery/module_resolution/
  load_libs split — T0.2 follow-up.
- Sweeping fs probe sites in `crates/tsz-cli/src/driver/sources.rs`
  and `bin/tsz_server/`. The plan asks specifically for resolver
  counters; broader fs instrumentation is a `CountingFs` trait
  redesign that should follow once Tier 2.0 is promoted (currently
  deferred per the 2026-05-10 decision).

## No conformance / behavior impact

Pure instrumentation. No checker/solver/parser semantics change.
