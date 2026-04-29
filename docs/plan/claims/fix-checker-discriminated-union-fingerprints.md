# fix(checker): align discriminated union TS2339 fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/checker-discriminated-union-fingerprints`
- **PR**: #1797
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the quick-picked fingerprint-only conformance failure
`TypeScript/tests/cases/conformance/types/union/discriminatedUnionTypes2.ts`.
TSZ currently emits the correct diagnostic codes (`TS2339`, `TS2353`) but the
wrong TS2339 fingerprints: it misses tsc's generic-literal union diagnostic for
`x.b` in `f14` and instead emits an extra `Property 'value' does not exist on
type 'never'` diagnostic in the unreachable `foo1` branch.

## Files Touched

- `crates/tsz-solver/src/narrowing/discriminants.rs`
  — keep generic discriminant property members as possible matches because
  a type parameter can instantiate to the compared literal.
- `crates/tsz-checker/src/types/property_access_type/resolve.rs`
  — suppress property-on-`never` diagnostics in unreachable branches when the
  receiver's declared/no-flow type still has the property.

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter discriminatedUnionTypes2 --verbose`
