# Claim: Add direct actual-lib Intl interface outcome counters

Date: 2026-05-14

## Claim

Add a stable perf-counter surface that classifies direct actual-lib Intl
interface routing outcomes, so future misses can be attributed to one of a
small set of explicit gates without code archaeology.

## Evidence

- `crates/tsz-common/src/perf_counters.rs`
  - adds `DirectActualLibIntlInterfaceOutcome` enum + stable name array
  - adds `direct_actual_lib_intl_interface_outcome` atomics to `PerfCounters`
  - adds recorder helper, snapshot field, JSON serialization, and text dump block
  - adds schema/order tests for `direct_actual_lib_intl_interface_outcomes`
- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - records Intl interface outcomes at the relevant direct-path gates
- `docs/plan/perf-runs/2026-05-14-direct-actual-lib-intl-interface-outcomes.md`
  - captures monorepo-006 attribution evidence and observed counts

## Validation

- `cargo test -p tsz-common direct_actual_lib_intl_interface_outcomes_locks_to_names_array -- --nocapture`
- `cargo test -p tsz-common snapshot_serializes_with_expected_top_level_keys -- --nocapture`
- `cargo test -p tsz-common classification_arrays_propagate_atomic_state_into_snapshot -- --nocapture`
- `cargo test -p tsz-checker --lib cross_file_direct -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests optional_tuple_generic_param_accepts_required_undefined_union_tuple -- --nocapture`
- `cargo test -p tsz-checker --test required_constraint_local_alias_tests -- --nocapture`
- `TSZ_PERF_COUNTERS=1 <tsz-perf-build>/tsz --noEmit -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-direct-actual-lib-intl-interface-outcomes-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-direct-actual-lib-intl-interface-outcomes-monorepo-006-pc.json` (expected exit `2`)
