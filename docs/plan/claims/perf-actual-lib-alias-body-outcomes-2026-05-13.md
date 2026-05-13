# Claim: Actual-lib alias body outcome counters

Date: 2026-05-13
Status: ready

## Claim

The next alias performance slice should be instrumentation-only: classify why
the actual-lib alias-body helper accepts or rejects each bundled-lib alias
before admitting more aliases.

This PR adds `direct_actual_lib_alias_body_outcomes` to the perf-counter text
and JSON snapshot. The field records success, conservative name-gate rejection,
value merges, unproven actual-lib declarations, resolver/definition-store proof
failures, missing alias bodies, and generic-alias rejection.

## Scope

- Do not widen the alias allowlist.
- Keep `PropertyKey` and generic utility aliases on fallback.
- Preserve the existing `PerfCounterSnapshot` stable named-array shape:
  `{ "name": ..., "count": ... }`.
- Wire the counter at every return point in `direct_actual_lib_type_alias_body`.

## Evidence

- `crates/tsz-common/src/perf_counters.rs`
  - adds `DirectActualLibAliasBodyOutcome` and stable bucket names.
  - adds `direct_actual_lib_alias_body_outcomes` to `PerfCounterSnapshot`.
  - includes the field in the text dump.
  - locks the array shape and atomic propagation in unit tests.
- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - records one outcome per actual-lib alias-body helper attempt.
  - does not change the admitted alias set.

## Validation

- `cargo test -p tsz-common direct_actual_lib_alias_body_outcomes_locks_to_names_array -- --nocapture`
- `cargo test -p tsz-common snapshot_serializes_with_expected_top_level_keys -- --nocapture`
- `cargo test -p tsz-common classification_arrays_propagate_atomic_state_into_snapshot -- --nocapture`
- `cargo test -p tsz-common cross_file_cache_miss_cause_atomic_propagates_into_snapshot -- --nocapture`
- `cargo test -p tsz-common source_file_symbol_arena_cache_eligibility_atomic_propagates_into_snapshot -- --nocapture`
- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type -- --nocapture`
