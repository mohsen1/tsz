# [WIP] fix(parser): align incomplete constructor type annotation diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/parser-incomplete-constructor-type-annotation`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-05 13:16:10 UTC

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/constructorWithIncompleteTypeAnnotation.ts`.
`tsz` currently reports `TS1435` and `TS1472` where `tsc` reports `TS1110`
and `TS1127` for an incomplete constructor type annotation. This PR will root
cause the parser or diagnostic recovery divergence, add a focused Rust
regression test in the owning crate, and verify the targeted conformance case.

## Files Touched

- `docs/plan/claims/fix-parser-incomplete-constructor-type-annotation.md`
- Production and regression-test files TBD after root-cause diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "constructorWithIncompleteTypeAnnotation" --verbose`.
- Planned: focused Rust regression test in the owning parser/checker crate.
- Planned: `cargo check`, focused `cargo nextest run`, conformance smoke, and full guarded conformance before marking ready.
