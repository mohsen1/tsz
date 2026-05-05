# [WIP] fix(conformance): suppress extra TS2344 in variance annotations

- **Date**: 2026-05-05
- **Branch**: `conformance/variance-annotations-extra-ts2344-20260505`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

The random conformance picker selected
`TypeScript/tests/cases/conformance/types/typeParameters/typeParameterLists/varianceAnnotations.ts`.
`tsc` reports the expected syntax, variance, and assignability diagnostics, but
`tsz` emits one extra TS2344. This PR will identify which variance-related
constraint check is too eager and route the fix through the owning checker or
solver boundary instead of filtering the diagnostic by test name.

## Files Touched

- TBD after diagnosis.

## Verification

- Planned: `cargo check --package tsz-checker`
- Planned: `cargo check --package tsz-solver`
- Planned: owning-crate `cargo nextest run` regression test
- Planned: `./scripts/conformance/conformance.sh run --filter "varianceAnnotations" --verbose`
- Planned: `./scripts/conformance/conformance.sh run --max 200`
