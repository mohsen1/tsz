# fix(checker): align discriminated union assignment fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-discriminated-union-assignment-fingerprints`
- **PR**: #3583
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

Claiming `TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatWithDiscriminatedUnion.ts`.

Current `origin/main` reports the expected TS2322 code, but the diagnostic
fingerprints differ. The `undefined` assignment displays the alias
`IAxisType` instead of the expected literal union, and an extra tuple-union
assignment diagnostic is emitted for the GH39357 case.

## Verification

- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --lib discriminated_union_object_literal_expands_literal_alias_property_target discriminated_tuple_union_accepts_literal_union_tuple_elements`
- `./scripts/conformance/conformance.sh run --filter "assignmentCompatWithDiscriminatedUnion" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
