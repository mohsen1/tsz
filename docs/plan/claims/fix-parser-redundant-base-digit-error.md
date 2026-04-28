# fix(parser): suppress duplicate parser-side digit-expected diagnostic for malformed prefixed integer literals

- **Date**: 2026-04-27
- **Branch**: `fix/parser-redundant-base-digit-error`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

For malformed prefixed integer literals like `0b21010` (`2` is not a binary digit), the scanner emits **TS1177** ("Binary digit expected") at the position of the offending character. tsc's `parseErrorAtPosition` then suppresses any subsequent same-position parser errors via its `lastError` slot. tsz emits a **redundant TS1005** at the same position (and was also re-emitting TS1177 from the parser, which dedups but bumps the scanner-water-mark, breaking dedup of subsequent TS1005). Concretely, `var binary = 0b21010;` produced **TS1005 (',' expected) + TS1005 (';' expected) + TS1177** instead of just **TS1177**.

Two-part fix:
1. Drop the redundant TS1125 / TS1177 / TS1178 parser-side emissions in `parse_numeric_literal`. The scanner already emits these at the same position with the same code; re-emitting from the parser only consumed the scanner-dedup slot.
2. Track the position of *any* dedup-suppressed parser error in `last_error_pos`, mirroring tsc's single `lastError` slot. Subsequent same-position parser errors (`',' expected`, `';' expected`) now suppress instead of leaking through.

Flips `invalidBinaryIntegerLiteralAndOctalIntegerLiteral.ts` from failing to passing.

## Files Touched

- `crates/tsz-parser/src/parser/state.rs` — added `last_error_pos == start` dedup branch + scanner-dedup branch updates `last_error_pos` (~15 LOC).
- `crates/tsz-parser/src/parser/state_expressions_literals.rs` — removed three duplicate base-digit-expected emission blocks (~46 LOC removed, ~12 LOC explanatory comment added).
- `crates/tsz-parser/tests/state_expression_tests.rs` — two unit tests:
  - `malformed_binary_literal_does_not_leak_ts1005_alongside_ts1177`
  - `malformed_octal_literal_does_not_leak_ts1005_alongside_ts1178`

## Verification

- `cargo nextest run -p tsz-parser` (697 tests pass, 2 new lock-in tests)
- `cargo nextest run -p tsz-checker --lib` (2944 tests pass)
- `./scripts/conformance/conformance.sh run --filter "invalidBinaryIntegerLiteralAndOctalIntegerLiteral"` → 1/1 pass (was 0/1)
- `./scripts/conformance/conformance.sh run --filter "parser"` → 787/794 (was 786/794)
- `./scripts/conformance/conformance.sh run --filter "binary"` → 79/80 pass (1 pre-existing baseline fail: `logicalOrOperatorWithTypeParameters.ts`)
