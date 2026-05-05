# [WIP] fix(checker): suppress extra recursive conditional diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/recursive-conditional-extra-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

The canonical picker selected
`TypeScript/tests/cases/compiler/recursiveConditionalTypes.ts`, currently an
only-extra conformance failure: `tsz` emits the expected `TS2322`, `TS2345`,
and `TS2589` diagnostics, but also emits extra `TS2339` and `TS2344`
diagnostics. This PR will identify the root cause in recursive conditional
evaluation, constraint handling, or diagnostic recovery, fix it in the owning
layer, and add a focused Rust regression test for the invariant.

## Files Touched

- TBD after investigation.

## Verification

- Planned: `cargo nextest run` for the owning crate test.
- Planned: `./scripts/conformance/conformance.sh run --filter "recursiveConditionalTypes" --verbose`.
