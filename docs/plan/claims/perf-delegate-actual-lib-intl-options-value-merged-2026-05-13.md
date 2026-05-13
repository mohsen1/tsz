# Claim: Admit narrow Intl options/registry interfaces in direct actual-lib path

Date: 2026-05-13

## Claim

The direct actual-lib interface path can safely admit a narrow Intl
options/registry interface family through namespace-qualified lookup, reducing
declaration-file `DelegateCrossArenaSymbol` child-checker constructions without
diagnostic drift.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - extends `is_direct_actual_intl_lib_interface_name` for:
    `DateTimeFormatOptions`, `NumberFormatOptions`,
    `NumberFormatOptionsCurrencyDisplayRegistry`,
    `NumberFormatOptionsStyleRegistry`,
    `NumberFormatOptionsUseGroupingRegistry`.
  - admits the same names in `is_direct_actual_lib_value_interface_name`
    for value-merged gating.
  - updates `direct_actual_lib_symbol_type_handles_selected_value_interfaces`
    to cover this admitted set.
- `docs/plan/perf-runs/2026-05-13-delegate-actual-lib-intl-options-value-merged.md`
  records monorepo-006 attribution evidence:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` children `24 -> 18`
  - `delegate.misses` `24 -> 18`
  - `checker.with_parent_cache_constructed` `24 -> 18`
  - declaration-file interface miss bucket `10 -> 3`.

## Validation

- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type_handles_selected_value_interfaces -- --nocapture`
- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type -- --nocapture`
- `cargo test -p tsz-checker --lib cross_file_direct -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests test_ts2322_flatarray_assignment_keeps_rhs_declared_alias_display -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_module_exports_define_property_does_not_fall_back_to_lib_signature -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_type_literal_bare_uint8array_does_not_poison_later_defaulted_refs -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_typed_array_to_locale_string_uses_options_parameter_type -- --nocapture`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
- `cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 .target/release/tsz --noEmit -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json /tmp/tsz-perf-goal-next/intl-options-after-diag.json --perf-counters-json /tmp/tsz-perf-goal-next/intl-options-after-pc.json` (expected exit `2`)

Known environment limitation:

- `cargo test -p tsz-checker --test generic_alias_assignability_pollution_tests -- --nocapture`
  still fails in this worktree due missing compiled TypeScript lib fixture setup
  for that test harness (`load_compiled_lib_files` cannot resolve
  `scripts/node_modules/typescript/lib` here).
