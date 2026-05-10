# perf(cli): gate-once the remaining inline resolver counters

- **Date**: 2026-05-10
- **Branch**: `perf/t0-resolver-inline-counter-gate-2026-05-10`
- **PR**: #5000
- **Status**: ready
- **Workstream**: 4.T0.3 (resolver filesystem attribution)

## Intent

PR #4966 introduced gate-once-cached counter wrappers for the three
filesystem probes (`count_is_file`, `count_is_dir`, `count_read_dir`)
and swept 21 call sites onto them. A Copilot review on that PR flagged
that *three other* inline `inc(&counters().X)` sites remained:

- `resolve_module_with_paths` virtual-root candidate path emission
  in the path-mapping branch.
- `extension_candidates` suffix-extension candidate emission.
- `read_package_json_uncached` per-package reads.

Each pays an unconditional `OnceLock<PerfCounters>` deref to compute
the field reference, even though `inc()` short-circuits on
`enabled_fast()` internally. The wrappers from #4966 fixed the same
shape of issue for the fs-probe trio; this PR extends the pattern to
the remaining three.

## Files Touched

- `crates/tsz-cli/src/driver/resolution.rs` (~40 LOC change):
  adds `count_candidate_path()` and `count_read_package_json()`, then
  routes the two candidate-emission sites and the uncached package JSON
  read site through them.
- `docs/plan/claims/perf-t0-resolver-inline-counter-gate-2026-05-10.md`:
  records the implementation claim and verification.

Both wrappers share the same gate-then-deref shape as the existing
fs-probe wrappers: `if enabled_fast() { inc(&counters().X) }`. Disabled
mode collapses to one cached-bool load; enabled mode pays one
`counters()` deref per increment.

No semantic change. Counter values for these fields under
`TSZ_PERF_COUNTERS=1` are unchanged. The disabled-mode cost drops by
one `OnceLock<PerfCounters>::get()` per call site invocation.

## Verification

- `cargo check -p tsz-cli` clean
- `cargo fmt --check` clean
- `cargo nextest run -p tsz-core test_check_files_parallel_jsdoc_import_type_default_namespace_emits_ts2352 --profile ci --no-capture`
  passed locally after the failing CI `unit` log was inspected
