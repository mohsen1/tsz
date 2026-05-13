# Claim: Admit Intl NumberFormatOptionsSignDisplayRegistry direct interface path

Date: 2026-05-13

## Claim

Admitting `NumberFormatOptionsSignDisplayRegistry` in the existing narrow Intl
namespace-qualified direct actual-lib interface path reduces declaration-file
`DelegateCrossArenaSymbol` residue by one more interface child-checker
construction with unchanged diagnostics.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - extends `is_direct_actual_intl_lib_interface_name` with
    `NumberFormatOptionsSignDisplayRegistry`.
  - extends `is_direct_actual_lib_value_interface_name` with
    `NumberFormatOptionsSignDisplayRegistry`.
  - updates `direct_actual_lib_symbol_type_handles_selected_value_interfaces`
    coverage.
- `docs/plan/perf-runs/2026-05-13-delegate-actual-lib-intl-sign-display-registry.md`
  records monorepo-006 attribution evidence:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` children `18 -> 17`
  - `delegate.misses` `18 -> 17`
  - `checker.with_parent_cache_constructed` `18 -> 17`.

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
- `TSZ_PERF_COUNTERS=1 .target/release/tsz --noEmit -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json /tmp/tsz-perf-goal-next/intl-sign-registry-after-diag.json --perf-counters-json /tmp/tsz-perf-goal-next/intl-sign-registry-after-pc.json` (expected exit `2`)
