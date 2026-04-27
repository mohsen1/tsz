# fix(scanner,parser): dedup TS1124/TS1351/TS1005 at empty-exponent literals

- **Date**: 2026-04-27
- **Time**: 2026-04-27 02:10:00
- **Branch**: `fix/scanner-exp-digit-dedup-20260427-0210`
- **PR**: pending
- **Status**: WIP
- **Workstream**: conformance / fingerprint parity

## Intent

`identifierStartAfterNumericLiteral.ts` was a fingerprint-only failure
because the scanner emitted TS1351 ("identifier cannot follow a numeric
literal") at the same position as the parser-emitted TS1124 ("Digit
expected") for `1ee`/`123ee`, and tsz then also emitted a stray TS1005 at
the same position. tsc's `parseErrorAtPosition` deduplicates by
`lastError.start`, so a follow-up error at the same position is suppressed.

## Root cause

- `scan_decimal_number` did not emit TS1124 inline when the exponent had no
  digits, so the position came from the parser at end-of-token (`3en` ->
  col 4 instead of col 3).
- Scanner-emitted diagnostics never participated in the parser's
  `parse_error_at` same-position dedup because they live in a separate
  vec drained at end-of-parse.

## Fix shape

1. Scanner-side: emit TS1124 inline in `scan_decimal_number`'s exponent
   branch when no digits are consumed; suppress the colliding TS1351 in
   `check_for_identifier_start_after_numeric_literal` when a scanner
   diagnostic at the same start was just pushed.
2. Parser-side: `parse_error_at` consults the most recent scanner
   diagnostic emitted *after* our last parser push (tracked via a
   high-water mark). Mirrors tsc's single-vec `lastError` semantics
   without changing diagnostic ordering or merge-time sort.
3. Removed the duplicated parser-level missing-exponent-digit emission
   that fired at the wrong position.

## Tests

- New scanner unit tests in `tsz-scanner/tests/scanner_impl_tests.rs`
  pinning the exact diagnostics for `1e`, `1e+`, `1ee`, `3en`, `1e9`.
- Existing `numeric_literal_exponent_tests.rs` coverage retained.
- `identifierStartAfterNumericLiteral.ts` flips from fingerprint-only
  failure to pass.
