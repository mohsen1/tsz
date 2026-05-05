# fix(checker): align logical-or type parameter diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-logical-or-type-parameter-diagnostics`
- **PR**: #3199
- **Status**: ready
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

- `crates/tsz-checker/src/types/computation/binary.rs`
- `crates/tsz-checker/tests/logical_operator_literal_preservation_tests.rs`
- `crates/tsz-solver/src/operations/binary_ops.rs`
- `crates/tsz-solver/src/diagnostics/format/compound.rs`

## Verification

- Baseline targeted conformance:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "logicalOrOperatorWithTypeParameters" --verbose`
  - Current result: fingerprint-only `TS2322`; expected expression-level
    `U | NonNullable<T>` and `V | NonNullable<U>` diagnostics, actual
    operand-level `T`/`U` and `U`/`V` diagnostics.
- Focused checker regression:
  - `CARGO_INCREMENTAL=0 cargo nextest run -p tsz-checker logical_or_type_parameter_assignment_reports_whole_expression`
  - Result: passed.
- Targeted conformance:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "logicalOrOperatorWithTypeParameters" --verbose`
  - Result: 1/1 passed, no fingerprint-only deltas.
- Conformance smoke:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --max 200`
  - Result: 200/200 passed, no fingerprint-only deltas.
