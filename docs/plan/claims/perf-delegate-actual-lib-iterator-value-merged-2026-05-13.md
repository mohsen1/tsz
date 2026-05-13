# Claim: Narrow value-merged iterator admission reduces declaration-file delegate residue

Date: 2026-05-13

## Claim

`direct_actual_lib_symbol_type` can safely admit a narrow value-merged
actual-lib interface slice (`Iterator` and `IteratorObject`) via
`resolve_lib_type_with_params`, reducing declaration-file
`DelegateCrossArenaSymbol` child-checker constructions without changing
diagnostics.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - adds `Iterator` and `IteratorObject` to
    `should_resolve_actual_lib_interface_with_params`.
  - adds `is_value_merged_actual_lib_interface_admitted` and only bypasses the
    value-merged guard for that iterator pair.
  - adds
    `direct_actual_lib_symbol_type_handles_iterator_interfaces_with_params`.
- `docs/plan/perf-runs/2026-05-13-delegate-actual-lib-iterator-value-merged.md`
  records monorepo-006 attribution evidence:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` declaration-file children `22 -> 20`
  - `delegate.misses` `24 -> 20`
  - `checker.with_parent_cache_constructed` `25 -> 20`.
  - counters-off timing was noisy across two local pairs, so this claim is not
    a wall-clock timing win claim.

## Validation

- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type -- --nocapture`
- `cargo test -p tsz-checker --lib cross_file_direct -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests test_ts2322_flatarray_assignment_keeps_rhs_declared_alias_display -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_module_exports_define_property_does_not_fall_back_to_lib_signature -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_type_literal_bare_uint8array_does_not_poison_later_defaulted_refs -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_typed_array_to_locale_string_uses_options_parameter_type -- --nocapture`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
- `cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 .target-perf-iterator-rebased/release/tsz --project scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --pretty false --noEmit --diagnostics-json /tmp/tsz-perf-iterator-56518-20260513/diag.json --perf-counters-json /tmp/tsz-perf-iterator-56518-20260513/pc.json` (expected exit `2`)
- `.target-perf-iterator-rebased/release/tsz --project scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --pretty false --noEmit --diagnostics-json /tmp/tsz-timing-iterator-56518-20260513/diag.json` (expected exit `2`; counters-off timing sample)

Known environment limitation:

- `cargo test -p tsz-checker --test generic_alias_assignability_pollution_tests -- --nocapture`
  currently fails in this worktree because `load_compiled_lib_files(...)` cannot
  find `scripts/node_modules/typescript/lib/*` (test dependency absent in this
  environment), so it is not a usable gate here.
