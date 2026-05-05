# [WIP] fix(conformance): avoid false TS2590 on conditional large-union access

- **Date**: 2026-05-05
- **Branch**: `conformance/conditional-large-union-ts2590-20260505`
- **PR**: #3208
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

The random conformance picker selected
`TypeScript/tests/cases/compiler/conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable.ts`.
`tsc` accepts this performance-oriented conditional-type case, but `tsz` emits
an extra TS2590 ("Expression produces a union type that is too complex to
represent."). This PR will identify the root cause of the false complexity
diagnostic and fix it in the owning semantic layer rather than suppressing the
diagnostic at the conformance boundary.

## Files Touched

- TBD after diagnosis.

## Verification

- Planned: `cargo check --package tsz-checker`
- Planned: `cargo check --package tsz-solver`
- Planned: `cargo build --profile dist-fast --bin tsz`
- Planned: owning-crate `cargo nextest run` regression test
- Planned: `./scripts/conformance/conformance.sh run --filter "conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable" --verbose`
- Planned: `./scripts/conformance/conformance.sh run --max 200`
