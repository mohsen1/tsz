# [WIP] fix(checker): align object element-access diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-object-element-access-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-05

## Intent

Fix the validated random conformance pick
`TypeScript/tests/cases/compiler/objectCreationOfElementAccessExpression.ts`.
`tsz` reports the expected `TS2348`, `TS2538`, and `TS2564` codes, but the
diagnostic fingerprints diverge from `tsc`: the missing fingerprints are the
`TS2348` non-callable constructor diagnostic and the `TS2538` invalid index-type
diagnostic on the malformed element-access expression.

## Files Touched

- `docs/plan/claims/fix-checker-object-element-access-diagnostics.md`
- Production and regression-test files TBD after root-cause diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "objectCreationOfElementAccessExpression" --verbose`.
- Planned: focused Rust regression test in the owning crate.
- Planned: `cargo check`, focused `cargo nextest run`, and conformance smoke before marking ready.
