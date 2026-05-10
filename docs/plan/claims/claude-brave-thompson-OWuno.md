# fix(checker): drill into array-literal elements for satisfies elaboration

- **Date**: 2026-05-10
- **Time**: 2026-05-10 07:30:00
- **Branch**: `claude/brave-thompson-OWuno`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance & Fingerprints)

## Intent

When a `satisfies` expression's source is an array literal whose elements
don't all match the target's element type, tsc's `elaborateElementwise`
emits `TS2322` at each offending element rather than `TS1360` on the whole
satisfies expression. tsz previously only drilled into object-literal
sources, so an array source like `[10, "20"] satisfies number[]` produced
a single generic `TS1360 Type '(string | number)[]' does not satisfy the
expected type 'number[]'` instead of the expected `TS2322 Type 'string'
is not assignable to type 'number'` at the `"20"` element.

This change generalises `check_satisfies_assignable_or_report` to also
elaborate array literal sources by routing through the existing
`try_elaborate_assignment_source_error` boundary helper, which dispatches
to `try_elaborate_array_literal_elements` for `ARRAY_LITERAL_EXPRESSION`
sources. When the element-wise elaboration produces at least one TS2322,
the generic TS1360 is suppressed, matching tsc.

## Files Touched

- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
  (additive ARRAY_LITERAL_EXPRESSION branch in
  `check_satisfies_assignable_or_report`)
- `crates/tsz-checker/src/tests/dispatch_tests.rs`
  (two new unit tests locking the invariant + non-regression on
  fully-compatible array literals)

## Verification

- `cargo nextest run -p tsz-checker --lib` (clean)
- `./scripts/conformance/conformance.sh run --filter typeSatisfaction_errorLocations1 --verbose`
  fingerprint diff drops from `3 missing + 1 extra` to `2 missing + 0 extra`
  (TS2322 element drill-in lands; the remaining 2 missing TS2345 fingerprints
  are a separate generic-T constraint issue, not in scope for this PR).
- `scripts/session/verify-all.sh` (no regressions)
