# [WIP] fix(checker): align mapped type constraint fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-mapped-type-constraint-fingerprint`
- **PR**: #2807
- **Status**: claim
- **Workstream**: 1 (Conformance - mapped type constraint diagnostics)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/conformance/types/mapped/mappedTypeConstraints2.ts`,
a fingerprint-only TS2322 failure. The live mismatch shows `tsz` emits the
right diagnostic code but prints anonymous mapped-type bodies and broad
`[string]` indexed-access displays where `tsc` preserves alias/index forms
such as `Mapped2<K>[`get${K}`]`, `Foo<T>[`get${T}`]`, and
`ObjectWithUnderscoredKeys<K>[`_${K}`]`.

This PR will root-cause the indexed-access/mapped-type display path and align
the TS2322 fingerprints with `tsc`, with a focused checker or solver
regression test for the invariant.

## Files Touched

- TBD after implementation.

## Verification

- `./scripts/conformance/conformance.sh run --filter "mappedTypeConstraints2" --verbose` (currently fingerprint-only on `origin/main`)
