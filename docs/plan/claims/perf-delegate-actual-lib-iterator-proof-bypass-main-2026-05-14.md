# Claim: Iterator proof-bypass direct actual-lib interface slice (main-based)

Date: 2026-05-14

## Claim

A narrow iterator-focused direct actual-lib follow-up removes declaration-file
interface residue and reduces `DelegateCrossArenaSymbol` misses on monorepo-006
relative to latest `main`, with unchanged diagnostics.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - admits iterator-family names in the parameterized direct-lib interface path.
  - allows an `Iterator` fallback to `resolve_lib_interface_type_by_symbol`.
  - adds an `Iterator` declaration-proof bypass under existing actual-lib
    provenance checks.
  - admits value-merged `Iterator` / `IteratorObject` in the direct value
    interface gate.
  - adds unit coverage for iterator direct lowering without declaration-arena proof.
- `docs/plan/perf-runs/2026-05-14-delegate-actual-lib-iterator-proof-bypass-main.md`
  records monorepo-006 attribution evidence:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` children `22 -> 19`
  - `delegate.misses` `24 -> 19`
  - `checker.with_parent_cache_constructed` `25 -> 19`
  - declaration-file residue rows removed: `Iterator`, `IteratorObject`, `Symbol`.

## Validation

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --lib cross_file_direct -- --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo clippy -p tsz-checker --lib -- -D warnings`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test ts2322_tests test_ts2322_flatarray_assignment_keeps_rhs_declared_alias_display -- --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test conformance_issues test_module_exports_define_property_does_not_fall_back_to_lib_signature -- --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test conformance_issues test_type_literal_bare_uint8array_does_not_poison_later_defaulted_refs -- --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test conformance_issues test_typed_array_to_locale_string_uses_options_parameter_type -- --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 .target/release/tsz --noEmit -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json /tmp/tsz-perf-main-micro2/iterator-main-after-diag.json --perf-counters-json /tmp/tsz-perf-main-micro2/iterator-main-after-pc.json` (expected exit `2`)
