# fix(solver): reduce keyof for nullish operands; preserve `keyof (T & {})` undistributed

- **Date**: 2026-04-28
- **Branch**: `fix/keyof-display-null-and-intersection-with-empty`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`unknownControlFlow.ts` is a fingerprint-only failure: codes match, message
text differs in two ways the printer can fix without touching evaluation:

1. `keyof null` (and `keyof undefined` / `keyof void`) should print as `never`.
   The evaluator already maps these to `TypeId::NEVER`; the printer
   short-circuits before reduction. Fix: when the operand is a nullish
   intrinsic, evaluate eagerly and recurse.
2. `keyof (T & {})` currently distributes to `keyof T | keyof {}`. The
   existing comment in `format/mod.rs` already states the tsc rule —
   "preserve undistributed when any member is a structural object or
   intrinsic" — but the implementation distributes unconditionally. Fix:
   add the structural-member guard so `keyof (T & {})` and similar stay
   intact in error text.

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/mod.rs` (~40 LOC)
- `crates/tsz-solver/src/diagnostics/format/tests.rs` or sibling unit-test
  module (unit tests for both rules)

## Verification

- `cargo nextest run -p tsz-solver --lib` (covers format unit tests)
- `./scripts/conformance/conformance.sh run --filter "unknownControlFlow" --verbose`
  (target reduces from 6 mismatches to ≤4)
