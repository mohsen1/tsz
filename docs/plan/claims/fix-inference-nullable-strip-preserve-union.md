# fix(solver): preserve nullable-stripped union from object-property inference

- **Date**: 2026-04-26
- **Branch**: `fix/inference-nullable-strip-preserve-union`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

`widenToAny1.ts` (and similar `{ x: T; y: T }` calls where one property is
nullable and another is a literal) was inferring `T = undefined` instead of
TSC's `T = string | undefined`. The "first-property-wins" fallback in
`resolve_from_candidates` was collapsing the union returned from
`get_common_supertype_for_inference` (which strips nullables, runs the
tournament, and re-attaches nullable members) back to a single member,
producing the wrong inferred type and the wrong TS2322 message.

This PR teaches the fallback to skip when any candidate is a nullable type
(`UNDEFINED`, `NULL`, or `VOID`). In that case the resulting union is the
correct output of TSC's `getCommonSupertype` + `getNullableType` pipeline,
not a fallback union from incompatible candidates.

## Files Touched

- `crates/tsz-solver/src/inference/infer_resolve.rs` (~17 LOC change in
  `resolve_from_candidates`)
- `crates/tsz-solver/tests/infer_tests.rs` (+90 LOC, two regression tests)

## Verification

- `cargo nextest run -p tsz-solver --lib` — 5451 tests pass
- `./scripts/conformance/conformance.sh run --filter widenToAny1` — 1/1 pass
- `./scripts/conformance/conformance.sh run --filter undefinedArgumentInference` — 1/1 pass
- `./scripts/conformance/conformance.sh run --filter genericCallWithObjectLiteral` — 2/2 pass
- `./scripts/conformance/conformance.sh run --filter widenedTypes` — 6/6 pass
