# fix(solver): combined-signature approach for union construct signatures

- **Date**: 2026-04-26
- **Branch**: `claude/exciting-keller-6PLSu`
- **PR**: #1520
- **Status**: ready
- **Workstream**: Conformance — fingerprint parity

## Intent

`resolve_union_new` previously returned Success if ANY union member's construct
signature accepted the arguments. TypeScript requires ALL members to succeed
(strict semantics), validated through a "combined union signature" that
intersects parameter types (contravariant) and unions return types.

The fix mirrors the Phase 1/2/3 approach already used by `resolve_union_call`:

- Phase 1: arity check against combined.min\_required / combined.max\_allowed
  (max of all members' required arg counts, respecting rest params)
- Phase 2: per-member resolution to collect return types
- Phase 3: validate each arg against combined.param\_types (intersected across
  members), reporting the correct arg index and raw expected type

Also adds `try_compute_combined_union_construct_signature` (mirrors the call
version) and strips `| undefined` from optional params before intersection so
error messages show the raw type (`number`, not `number | undefined`).

Fixes `unionTypeConstructSignatures.ts` from fingerprint-only failure to 100%
pass, resolving 15 missing fingerprints and 8 spurious extra fingerprints
(wrong arg index, wrong expected type, wrong arg count messages).

## Files Touched

- `crates/tsz-solver/src/operations/core/call_evaluator.rs` — promote
  `CombinedUnionSignature` from `pub(super)` to `pub(crate)`
- `crates/tsz-solver/src/operations/core/call_resolution.rs` — add
  `try_compute_combined_union_construct_signature` (~130 LOC)
- `crates/tsz-solver/src/operations/constructors.rs` — rewrite
  `resolve_union_new` with Phase 1/2/3 approach; remove
  `compute_combined_construct_return` helper (~120 LOC change)
- `crates/tsz-solver/tests/operations_tests.rs` — 4 new unit tests

## Verification

- `cargo test -p tsz-solver --lib -- test_union_new` (4 tests pass)
- `cargo test -p tsz-solver --lib` (5529 tests pass)
- `conformance --filter unionTypeConstructSignatures`: 1/1 pass (was 0/1)
- `conformance --filter unionType`: 26/30 (was 25/30)
- `conformance --filter construct`: 112/113 (no regression)
