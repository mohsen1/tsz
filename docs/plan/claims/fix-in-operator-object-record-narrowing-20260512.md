# [WIP] fix(checker): preserve object record narrowing after `in`

- **Date**: 2026-05-12
- **Branch**: `fix-in-operator-object-record-narrowing-20260512`
- **Issue**: #5970
- **Status**: claim
- **Workstream**: 1 (diagnostic conformance / false-positive narrowing)

## Intent

Make `tsz` match `tsc` when an `unknown` value is narrowed through
`typeof x === "object"`, `x !== null`, and a string-literal `in` check. The
positive `in` branch should preserve an object-with-property shape so
subsequent property access does not see `never`.

## Planned Scope

- Flow/solver narrowing for `in` operator checks on object-like sources.
- Focused regression coverage for `unknown` to `object & Record<K, unknown>`.

## Verification Plan

- Focused checker or solver test for #5970.
- `env CARGO_INCREMENTAL=0 cargo test ...` for the owning crate target.
- `env CARGO_INCREMENTAL=0 cargo check ...` for touched crate(s).
- Manual `tsc` vs `.target/release/tsz` comparison for the issue repro.
