# fix(checker): reject unique symbol in conditional extends

- **Date**: 2026-05-12
- **Branch**: `fix-unique-symbol-extends-ts1335-20260512`
- **PR**: #5911
- **Status**: ready
- **Workstream**: Conformance

## Intent

Emit TS1335 for `unique symbol` used directly as the extends type in conditional types, matching tsc and removing the downstream false TS2322 described in #5833.
The implementation reuses the shared arena-only unique-symbol inspectors so conditional type validation stays aligned with the other unique-symbol recognition paths.

## Files Touched

- `crates/tsz-checker/src/state/state_checking_members/member_declaration_checks.rs`
- `crates/tsz-checker/src/types/unique_symbol_arena.rs`
- `crates/tsz-checker/src/types/mod.rs`
- `crates/tsz-checker/tests/ts1338_tests.rs`

## Verification

- `cargo fmt --all`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker unique_symbol_in_conditional_extends -- --nocapture` (2 matching tests passed)
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker parenthesized_unique_symbol_in_conditional_extends -- --nocapture` (1 matching test passed)
- `CARGO_TARGET_DIR=/private/tmp/tsz-target-5911 CARGO_BUILD_JOBS=1 cargo fmt --check`
- `CARGO_TARGET_DIR=/private/tmp/tsz-target-5911 CARGO_BUILD_JOBS=1 cargo clippy -p tsz-checker --lib -- -D warnings`
- `CARGO_TARGET_DIR=/private/tmp/tsz-target-5911 CARGO_BUILD_JOBS=1 cargo nextest run -p tsz-checker -E 'test(unique_symbol_in_conditional_extends) | test(parenthesized_unique_symbol_in_conditional_extends)' --no-fail-fast` (2 passed)
