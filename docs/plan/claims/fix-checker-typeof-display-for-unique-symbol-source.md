# fix(checker): preserve `typeof <expr>` source display for unique-symbol property accesses

- **Date**: 2026-04-26
- **Branch**: `fix/checker-typeof-display-for-unique-symbol-source`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 — Diagnostic Conformance / fingerprint-only TS2322

## Intent

`symbolType2.ts` (`"" in Symbol.toPrimitive`) emitted
`Type 'symbol' is not assignable to type 'object'.` because tsz widened the
unique-symbol-typed source to its primitive `symbol` for display. tsc shows
`Type 'typeof Symbol.toPrimitive' is not assignable to type 'object'.` —
preserving the property-access form when the value type is `unique symbol`.

This PR adds a small typeof-preservation branch at the top of
`format_assignment_source_type_for_diagnostic`: when the source expression
is a property/element access whose type is `UniqueSymbol`, format as
`typeof <expr-text>` (with trailing `;` stripped to handle expression
statements like `"" in X.y;`).

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs` —
  ~25 LOC: new `typeof_unique_symbol_source_display` helper + an early
  call from `format_assignment_source_type_for_diagnostic`.
- `crates/tsz-checker/tests/typeof_unique_symbol_source_display_tests.rs` —
  2 unit tests (element-access typeof rendering + identifier-source
  no-widen invariant).
- `crates/tsz-checker/src/lib.rs` — register the test module.

## Verification

- `cargo nextest run -p tsz-checker --lib typeof_unique_symbol_source_display`
  (2 pass)
- `./scripts/conformance/conformance.sh run --filter "Symbols/symbolType2.ts"`
  (PASS — was fingerprint-only FAIL)
- `./scripts/conformance/conformance.sh run --filter "Symbols/symbol"` (95/95
  pass — no regressions)
- Full conformance run: `12188 → 12189 (+1)`, no regressions
