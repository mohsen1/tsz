# fix(checker): align logical-or type parameter diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-logical-or-type-parameter-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance divergence in
`TypeScript/tests/cases/conformance/expressions/binaryOperators/logicalOrOperator/logicalOrOperatorWithTypeParameters.ts`.

`tsc` reports one `TS2322` diagnostic at the annotated variable declaration
when assigning `t || u` or `u || v` to `{}`. `tsz` currently emits separate
operand-level `TS2322` diagnostics for each type parameter branch (`T`/`U` and
`U`/`V`). This slice will align the logical-or expression type used for the
assignment diagnostic without weakening ordinary operand checking.

## Files Touched

- TBD

## Verification

- Baseline targeted conformance:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "logicalOrOperatorWithTypeParameters" --verbose`
  - Current result: fingerprint-only `TS2322`; expected expression-level
    `U | NonNullable<T>` and `V | NonNullable<U>` diagnostics, actual
    operand-level `T`/`U` and `U`/`V` diagnostics.
