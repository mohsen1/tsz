# fix(checker): contextual typing for rest-tuple callback params

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-QsP3q`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

Fix two bugs in contextual parameter-type inference for callbacks against
rest-tuple signatures (`...args: [A, B, C]`):

1. **False-positive TS2345** when a callback has more regular params than
   elements in a fixed tuple, so the callback's `...rest` maps to an empty
   slice `[]`.  The old code returned the full tuple type for the rest
   param, causing an incorrect assignability failure.

2. **Wrong `any` type** for regular params before a rest param when the
   contextual signature has both fixed params and a rest param (e.g.
   `(x: number, ...args: T)`).  The old code looked up tuple element
   `[index]` from the rest tuple without subtracting `rest_start`, giving
   `any` instead of `number` for the first param.

Fixes the conformance test
`TypeScript/tests/cases/conformance/types/rest/restTuplesFromContextualTypes.ts`.

## Files Touched

- `crates/tsz-checker/src/types/utilities/core.rs` (~50 LOC change)
- `crates/tsz-checker/src/lib.rs` (test module registration)
- `crates/tsz-checker/tests/rest_tuple_contextual_typing_tests.rs` (new, 8 tests)

## Verification

- `cargo test -p tsz-checker --lib "rest_tuple"` — 8/8 pass
- `./scripts/conformance/conformance.sh run --filter "restTuplesFromContextualTypes"` — 1/1 pass (was failing before)
