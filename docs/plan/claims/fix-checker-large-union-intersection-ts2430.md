# fix(checker): align large-union intersection diagnostics

- **Date**: 2026-05-05 22:11:01 UTC
- **Branch**: `fix/checker-large-union-intersection-ts2430`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance - diagnostic pass-rate fix)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/intersectionsOfLargeUnions2.ts`. On current
`origin/main`, the focused runner still fails: `tsc` reports `TS2300`,
`TS2430`, and `TS2536`, while `tsz` reports `TS2300`, `TS2536`, and an extra
`TS2677`.

This PR will diagnose the root cause behind the missing lib inheritance
diagnostic and the extra type-predicate assignability diagnostic, fix it in the
owning solver/checker boundary layer, and add focused Rust regression coverage.

## Files Touched

- TBD after implementation.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --package tsz-checker --package tsz-solver`
- focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --filter "intersectionsOfLargeUnions2" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
