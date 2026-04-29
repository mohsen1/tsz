# fix(checker): align destructuring spread TS2339 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/checker-destructuring-spread-ts2339-fingerprint`
- **PR**: #1740
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fixes the fingerprint-only TS2339 conformance mismatch for
`TypeScript/tests/cases/conformance/es6/destructuring/destructuringSpread.ts`.
Nested object-literal spreads were carrying their source object's local
`declaration_order` into the containing literal, so display order collided
with later explicit properties and the TS2339 receiver type printed as
`{ c; f; d; e }` instead of tsc's `{ f; e; d; c }`.

## Files Touched

- `docs/plan/claims/fix-checker-destructuring-spread-ts2339-fingerprint.md`
- `crates/tsz-checker/src/types/computation/object_literal/computation.rs`
- `crates/tsz-checker/tests/spread_rest_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo check -p tsz-checker`
- `cargo nextest run -p tsz-checker --test spread_rest_tests` (69/69)
- `./scripts/conformance/conformance.sh run --filter "destructuringSpread" --verbose` (1/1)
- `./scripts/conformance/conformance.sh run --max 200` (200/200)
