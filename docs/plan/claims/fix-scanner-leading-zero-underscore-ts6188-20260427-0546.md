**2026-04-27 05:46:00**

# fix(scanner): emit TS6188 at `0_<rest>` opening to mirror tsc's `scanNumber`

## Branch
`fix/scanner-leading-zero-underscore-ts6188-20260427-0546`

## Scope
Add the missing `scanNumber` "leading zero followed by underscore" diagnostic
in `crates/tsz-scanner/src/scanner_impl.rs::scan_number`. tsc emits TS6188
("Numeric separators are not allowed here") at the underscore position when a
numeric literal opens with `0_`, before falling through to the standard
fragment scan that may then emit TS6189 for any consecutive separators.

Without this pre-check, our scanner missed TS6188 on shapes like `0__0.0e0`,
`0__0.0e+0`, `0__0.0e-0`, `0_0.5_5`, etc., even though we already handled
`0_<dot>` and `0_<EOF>` via the trailing-underscore rescue path in
`scan_digits_with_separators`.

## Targets
- `parser.numericSeparators.decmialNegative.ts` — fingerprint-only failure
  resolves (3 missing TS6188 fingerprints filled in).
- `numberLiteralsWithLeadingZeros.ts` — gains TS6188 codes (still missing
  TS1005, so this test does not fully pass yet, but the shape improves).

## Tests
Six new unit tests in `crates/tsz-scanner/src/scanner_impl.rs::tests` lock in:
- `0__0.0e0` → TS6188 at pos=1, TS6189 at pos=2.
- `0__0.0e+0` and `0__0.0e-0` → same shape.
- `0_0.5_5` → TS6188 at pos=1.
- `0_.0e0` → single TS6188 at pos=1 (dedup with trailing rescue).
- `1__0.0e0` → only TS6189 at pos=2 (no `0_` rule for non-zero leading).
- `0.0__0e0` → only TS6189 at pos=4 (no `0_` rule when `0.` precedes).
