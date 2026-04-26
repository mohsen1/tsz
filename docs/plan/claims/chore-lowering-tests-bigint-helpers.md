# chore(lowering/tests): unit tests for numeric/bigint normalization helpers

- **Date**: 2026-04-26 08:37:27
- **Branch**: `chore/lowering-tests-bigint-helpers`
- **PR**: #1332
- **Status**: ready
- **Workstream**: Test coverage (DRY/quality)

## Intent

Add focused unit tests for the three pure numeric helpers in
`crates/tsz-lowering/src/lower/advanced.rs`:
`strip_numeric_separators`, `bigint_base_to_decimal`, and
`normalize_bigint_literal`. These helpers feed `lower_literal_type` for
numeric/bigint literals (hex/binary/octal, separators, large values) and
have many edge cases (empty input, invalid digits, leading zeros, very
large bigints) that the existing end-to-end tests do not exercise. Direct
unit tests pin behavior so regressions are caught early.

The slice is pure-additive: tests only, no behavior change. Helpers are
`pub(super)` so the new tests live in an inline `#[cfg(test)]` module
inside `advanced.rs`.

## Files Touched

- `crates/tsz-lowering/src/lower/advanced.rs` (+415 LOC, additive only — inline `numeric_helper_tests` mod)
- `docs/plan/claims/chore-lowering-tests-bigint-helpers.md` (claim file)

## Verification

- `cargo nextest run -p tsz-lowering` → 153 passed (114 prior + 34 new helper tests)
- `cargo clippy -p tsz-lowering --all-targets` → clean
- 34 new tests cover: separator stripping (8), base-N → decimal (11),
  bigint literal normalization (15), including u64/u128 edge cases.
