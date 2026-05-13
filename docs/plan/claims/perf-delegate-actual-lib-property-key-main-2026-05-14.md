# Claim: Admit PropertyKey direct actual-lib alias body (main-based)

Date: 2026-05-14

## Claim

Admitting `PropertyKey` in the direct actual-lib alias-body allowlist removes
one declaration-file `DelegateCrossArenaSymbol` miss on monorepo-006 relative
to latest `main`, with unchanged diagnostics.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - extends `is_direct_actual_lib_alias_body_admitted` with `PropertyKey`.
  - updates the PropertyKey alias-body test to expect direct lowering success.
- `docs/plan/perf-runs/2026-05-14-delegate-actual-lib-property-key-main.md`
  records monorepo-006 attribution evidence:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` children `22 -> 21`
  - `delegate.misses` `24 -> 23`
  - `checker.with_parent_cache_constructed` `25 -> 24`
  - declaration-file residue row `PropertyKey` removed.

## Validation

- `cargo test -p tsz-checker --lib cross_file_direct -- --nocapture`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
- `cargo test -p tsz-checker --test ts2322_tests test_ts2322_flatarray_assignment_keeps_rhs_declared_alias_display -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_module_exports_define_property_does_not_fall_back_to_lib_signature -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_type_literal_bare_uint8array_does_not_poison_later_defaulted_refs -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_typed_array_to_locale_string_uses_options_parameter_type -- --nocapture`
- `cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 .target/release/tsz --noEmit -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json /tmp/tsz-perf-main-micro/propertykey-main-after-diag.json --perf-counters-json /tmp/tsz-perf-main-micro/propertykey-main-after-pc.json` (expected exit `2`)
