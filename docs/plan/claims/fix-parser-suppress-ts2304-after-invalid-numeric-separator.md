# fix(parser): suppress false TS2304 after invalid numeric separator

- **Date**: 2026-04-27
- **Branch**: `fix/loop-iter-2`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Inputs like `0_X0101` produce **TS6188** (separator-not-allowed) and **TS1351** (identifier-cannot-immediately-follow-numeric-literal) in tsc. tsz was additionally emitting a spurious **TS2304** ("Cannot find name 'X0101'") because `parse_numeric_literal` had a recovery branch that explicitly fired TS2304 when an identifier directly followed an invalid separator. tsc's conformance baselines disagree — the identifier is parser-recovery debris, not a real name-resolution candidate.

Removed the manual TS2304 emission. The flag-tracking and `invalid_separator_pos` accessor are kept (still used by the existing TS1351 emission and any future recovery work), but the false TS2304 path is gone.

Flips three tests from failing to passing:
- `parser.numericSeparators.binaryNegative.ts`
- `parser.numericSeparators.hexNegative.ts`
- `parser.numericSeparators.octalNegative.ts`

## Files Touched

- `crates/tsz-parser/src/parser/state_expressions_literals.rs` — removed the false-TS2304 emission branch (~16 LOC); replaced with an explanatory comment (~7 LOC).
- `crates/tsz-parser/tests/state_expression_tests.rs` — added `invalid_numeric_separator_followed_by_identifier_does_not_emit_ts2304` to lock the suppression.

## Verification

- `cargo nextest run -p tsz-parser` (692 tests pass, 1 new)
- `cargo nextest run -p tsz-checker --lib` (2942 tests pass)
- `./scripts/conformance/conformance.sh run --filter "numericSeparators"` → 8/9 (was 5/9)
- `./scripts/conformance/conformance.sh run --filter "parser"` → 789/794 (was 786/794, +3)
