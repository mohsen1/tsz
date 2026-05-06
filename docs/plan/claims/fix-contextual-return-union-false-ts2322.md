# [WIP] fix(checker): suppress false contextual return union TS2322

- **Date**: 2026-05-06
- **Branch**: `fix/contextual-return-union-false-ts2322`
- **PR**: #3483
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

The canonical picker selected
`TypeScript/tests/cases/compiler/inferenceContextualReturnTypeUnion3.ts`, a
false-positive conformance failure where `tsz` emits an extra `TS2322` while
`tsc` emits no diagnostics. I will root-cause the contextual return type /
union inference path, fix the owning layer, and add focused Rust regression
coverage before marking the PR ready.

## Files Touched

- TBD after investigation.

## Verification

- Planned: owning-crate `cargo nextest run`.
- Planned: `./scripts/conformance/conformance.sh run --filter "inferenceContextualReturnTypeUnion3" --verbose`.
