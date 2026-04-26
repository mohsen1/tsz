# fix(lowering): preserve unresolved identifier name in type position via UnresolvedTypeName

- **Date**: 2026-04-26
- **Branch**: `fix/lowering-unresolved-identifier-preserves-name-in-type-position`
- **PR**: #1464
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — fingerprint parity)

## Intent

`lower_identifier_type` returns `TypeId::ERROR` when a type-position identifier
fails to resolve to a known DefId. The printer renders `TypeId::ERROR` as the
bare token `error`, which leaks into TS2345 / TS2322 messages — e.g.
`Argument of type '(items: error) => void' is not assignable to parameter of type
'() => any'.` for `lambdaArgCrash.ts`. tsc preserves the syntactic name
(`(items: ItemSet) => void`).

The qualified-name path (`A.B`) in `lower_qualified_name_type` already falls
back to `interner.unresolved_type_name(name)` before returning
`TypeId::ERROR`. The bare-identifier path was missing this fallback —
asymmetric with the qualified-name path. This PR mirrors the same fallback in
`lower_identifier_type` so display agrees.

The TS2304 ("Cannot find name 'X'") diagnostic is unchanged — it is emitted
upstream in `type_node.rs` based on syntactic checks, independent of lowering.

## Files Touched

- `crates/tsz-lowering/src/lower/advanced.rs` (~9 LOC change — added the
  `UnresolvedTypeName` fallback before `TypeId::ERROR`)
- `crates/tsz-lowering/tests/lower_tests.rs` (~70 LOC — two new regression
  tests: identifier path preserves name; identifier and qualified-name paths
  agree)

## Verification

- `cargo nextest run -p tsz-lowering` (155 tests pass)
- `./scripts/conformance/conformance.sh run --filter "lambdaArgCrash"` (now
  PASSES; was fingerprint-only)
- `./scripts/conformance/conformance.sh run --max 1000` (995/1000 — no
  regressions vs main baseline)
