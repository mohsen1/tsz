# fix(checker): align JSX LibraryManagedAttributes fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-jsx-library-managed-attributes-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the remaining fingerprint-only TS2322 drift in
`TypeScript/tests/cases/conformance/jsx/tsxLibraryManagedAttributes.tsx`.
The random pick shows matching error codes and positions, but `tsz` still
prints less tsc-like target types for JSX `LibraryManagedAttributes` paths:
expanded `ReactNode` aliases, indexed-access prop-type aliases, and
`Defaultize` application arguments differ from `tsc`. This follows the prior
mapped-infer PR that left the parent fixture fingerprint-only with unrelated
display drift.

## Files Touched

- TBD after diagnosis.

## Verification

- `./scripts/conformance/conformance.sh run --filter "tsxLibraryManagedAttributes" --verbose` (baseline captured; currently fingerprint-only)
- Targeted unit tests and no-regression conformance runs will be added before marking ready.
