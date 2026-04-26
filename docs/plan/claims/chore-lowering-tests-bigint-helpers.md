# chore(lowering/tests): unit tests for numeric/bigint normalization helpers

- **Date**: 2026-04-26 08:37:27
- **Branch**: `chore/lowering-tests-bigint-helpers`
- **PR**: TBD
- **Status**: claim
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

- `crates/tsz-lowering/src/lower/advanced.rs` (~+150 LOC, additive only)
- `docs/plan/claims/chore-lowering-tests-bigint-helpers.md` (claim file)

## Verification

- `cargo nextest run -p tsz-lowering` (existing 114 tests pass + new tests pass)
