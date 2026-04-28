# Emit TS6188 for `0__<digit>` leading-zero double-separator pattern

- **Date**: 2026-04-28
- **Branch**: `fix/scanner-leading-zero-double-separator-ts6188`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`crates/tsz-scanner/src/scanner_impl.rs` line 1321: the leading-zero TS6188
rule (which fires for `0_<digit>` patterns) used `is_digit(pos+2)` as its
guard. That handles `0_0`, `0_1.5`, etc. — but misses `0__0` (consecutive
separators after the leading zero), where `pos+2` is another `_`. tsc
emits both TS6188 at the first `_` and TS6189 at the second `_`; tsz
emitted only TS6189.

Widen the guard to fire when `pos+2` is a digit OR another `_`. The inner
`scan_digits_with_separators` loop continues to emit TS6189 for the
consecutive-separator run; this fix adds the missing leading-zero TS6188
at the first `_`.

## Files Touched

- `crates/tsz-scanner/src/scanner_impl.rs` — widen the `pos+2` guard from
  `is_digit` to `is_digit || == UNDERSCORE` (~3 LOC + comment expansion);
  add lock-in unit test `separator_after_leading_zero_followed_by_double_underscore`.

## Verification

- `cargo nextest run -p tsz-scanner` → 355 pass (354 + 1 new).
- `cargo nextest run -p tsz-parser -p tsz-scanner` → 1050 pass.
- `./scripts/conformance/conformance.sh run --filter "decmialNegative"` →
  **1/1 pass** (was 0/1; flips PASS).
- `./scripts/conformance/conformance.sh run --filter "numericSeparators"` →
  9/9 pass (was 8/9).
- `./scripts/conformance/conformance.sh run --filter "numeric"` →
  33/33 pass (no regressions).
