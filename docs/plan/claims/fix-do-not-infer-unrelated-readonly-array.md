# [WIP] fix(checker): avoid unrelated readonly array inference error

- **Date**: 2026-05-05
- **Branch**: `fix/do-not-infer-unrelated-readonly-array`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/doNotInferUnrelatedTypes.ts`, where `tsc`
accepts `Array<LiteralType>` as a `ReadonlyArray<T>` argument but `tsz`
emits a false-positive `TS2345`.

## Files Touched

- `docs/plan/claims/fix-do-not-infer-unrelated-readonly-array.md`

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "doNotInferUnrelatedTypes" --verbose`
- Focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
