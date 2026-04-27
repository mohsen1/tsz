# fix(parser): emit TS1005 alongside TS1121/TS1489 for `00.5;` style literals

- **Date**: 2026-04-27
- **Branch**: `fix/parser-ts1005-after-leading-zero`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

For source like `00.5;` the scanner correctly tokenizes `00` (legacy octal `NumericLiteral`) followed by `.5` (decimal `NumericLiteral`). tsc emits both **TS1121** ("Octal literals are not allowed") at column 1 and **TS1005** ("';' expected") at column 3 because `parseErrorAtPosition` dedups only by exact start position. tsz was suppressing TS1005 because the parser's `should_report_error()` gate widens dedup to `abs_diff <= 3` source positions — the two errors are 2 apart and trip that gate.

Narrow fix in `parse_error_for_missing_semicolon_after`: when the most recent parse diagnostic is **TS1121** or **TS1489** (the leading-zero family) at a position different from the current `;`-anchor, allow TS1005 through. `parse_error_at`'s built-in same-position dedup still prevents same-spot duplicates.

This flips `numberLiteralsWithLeadingZeros.ts` from `diff=1` (missing TS1005) to a passing test.

## Files Touched

- `crates/tsz-parser/src/parser/state.rs` — added `last_error_was_leading_zero_at_other_pos()` helper; loosened the `should_report_error()` gate in `parse_error_for_missing_semicolon_after` for non-identifier expressions (~20 LOC change).
- `crates/tsz-parser/tests/state_expression_tests.rs` — added two unit tests:
  - `legacy_octal_with_decimal_part_emits_both_ts1121_and_ts1005` — locks the fix.
  - `decimal_with_leading_zero_and_decimal_part_does_not_emit_ts1005` — pins the negative case (`08.5;` is one decimal token, no TS1005).

## Verification

- `cargo nextest run -p tsz-parser` (693 tests pass, 2 new)
- `cargo nextest run -p tsz-checker --lib` (2942 tests pass)
- `./scripts/conformance/conformance.sh run --filter "numberLiterals"` → 1/1 pass (was 0/1)
- `./scripts/conformance/conformance.sh run --filter "parser"` → 786/794 (8 pre-existing fails in baseline, all `numericSeparators.*Negative` and `parser.asyncGenerators.objectLiteralMethods.es2018`)
