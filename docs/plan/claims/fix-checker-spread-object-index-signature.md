# Claim: Fix spread object assignability to index signatures

## Target

`TypeScript/tests/cases/compiler/spreadOfObjectLiteralAssignableToIndexSignature.ts`

Snapshot category: false-positive.

Expected diagnostics: none.

Actual diagnostics: TS2322.

## Plan

Investigate why a spread object literal that TypeScript accepts as assignable to an index-signature target is reported as TS2322, then tighten object-spread or assignability handling so the conformance file passes without dropping real excess/index-signature diagnostics.

## Verification

Planned:

- focused checker regression test
- filtered conformance for `spreadOfObjectLiteralAssignableToIndexSignature`
