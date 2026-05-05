# [WIP] fix(conformance): align NoInfer diagnostic fingerprints

- **Date**: 2026-05-05
- **Branch**: `conformance/noinfer-fingerprints-20260505`
- **PR**: #3342
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

The random conformance picker selected
`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/noInfer.ts`.
The code families match `tsc` (`TS2322`, `TS2345`, `TS2353`, `TS2741`), but
the diagnostic fingerprints differ. Direct CLI comparison shows at least one
missing property-context `TS2322` for `NoInfer<T>` nested inside an object
property and a stale-literal display drift in a `TS2345` missing-property
message. This PR will root-cause those NoInfer inference/display differences
through the owning checker/solver boundary instead of filtering diagnostics by
fixture.

## Files Touched

- TBD after diagnosis.

## Verification

- Planned: `cargo check --package tsz-solver`
- Planned: `cargo check --package tsz-checker`
- Planned: owning-crate regression test
- Planned: targeted `noInfer.ts` diagnostic comparison
- Planned: targeted conformance runner if the runner build is not externally killed
