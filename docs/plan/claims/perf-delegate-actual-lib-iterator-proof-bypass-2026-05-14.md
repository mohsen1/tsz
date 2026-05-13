# Claim: Allow Iterator direct actual-lib lowering without full declaration-arena proof

Date: 2026-05-14

## Claim

Allowing `Iterator` through the direct actual-lib path when declaration-arena
proof is unavailable, but bundled-lib provenance is already proven, removes the
last declaration-file interface `DelegateCrossArenaSymbol` residue on
monorepo-006 with unchanged diagnostics.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - adds `allow_actual_lib_declaration_proof_bypass` for `Iterator`.
  - uses this gate in `direct_actual_lib_symbol_type` so `Iterator` can keep
    the existing direct lowering path when `symbol_declarations_are_direct_actual_lib_only`
    returns false.
  - adds unit test
    `direct_actual_lib_symbol_type_allows_iterator_without_declaration_arena_proof`.
- `docs/plan/perf-runs/2026-05-14-delegate-actual-lib-iterator-proof-bypass.md`
  records monorepo-006 attribution evidence:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` children `16 -> 14`
  - `delegate.misses` `16 -> 14`
  - `checker.with_parent_cache_constructed` `16 -> 14`
  - `delegate_miss_classification.by_kind.interface` `1 -> 0`.

## Validation

- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type_handles_iterator_interfaces_with_params -- --nocapture`
- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type_allows_iterator_without_declaration_arena_proof -- --nocapture`
- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type -- --nocapture`
- `cargo test -p tsz-checker --lib cross_file_direct -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests test_ts2322_flatarray_assignment_keeps_rhs_declared_alias_display -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_module_exports_define_property_does_not_fall_back_to_lib_signature -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_type_literal_bare_uint8array_does_not_poison_later_defaulted_refs -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_typed_array_to_locale_string_uses_options_parameter_type -- --nocapture`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
- `cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 .target/release/tsz --noEmit -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json /tmp/tsz-perf-goal-next/iterator-decl-proof-bypass-after-diag.json --perf-counters-json /tmp/tsz-perf-goal-next/iterator-decl-proof-bypass-after-pc.json` (expected exit `2`)
