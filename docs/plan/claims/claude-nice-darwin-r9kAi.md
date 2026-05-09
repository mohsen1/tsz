# fix(wasm): wire TsTypeChecker predicates and TypeFlags through the solver

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-r9kAi`
- **PR**: TBD
- **Status**: claim
- **Workstream**: WASM API parity (issue #4742)

## Intent

The wasm `TsTypeChecker` predicate methods (`isUnionType`,
`isIntersectionType`, `isTypeParameter`, `isArrayType`, `isTupleType`)
all return hardcoded `false`, and `getTypeFlags` only recognizes a
handful of intrinsic ids; `isNullableType` only matches the bare `null`
and `undefined` intrinsics. JS callers therefore cannot classify any
non-intrinsic type that comes out of the wasm bridge.

This PR routes those methods through the existing
`tsz_solver::is_*_type` visitor predicates and rebuilds `getTypeFlags`
as a structural mapping over `TypeData` that mirrors the public
`TypeFlags` bit values from `typescript`. `isNullableType` is widened
to recognize unions that contain `null` or `undefined`, matching
`TypeChecker.isNullableType` in the TypeScript public API.

## Files Touched

- `crates/tsz-wasm/src/wasm_api/type_checker.rs` — predicate + flag
  rewiring, plus a `#[cfg(test)]`-gated test constructor and unit tests.

## Verification

- `cargo test -p tsz-wasm --lib` (all tests pass, including new ones)
- `cargo build -p tsz-wasm` (release build clean)
