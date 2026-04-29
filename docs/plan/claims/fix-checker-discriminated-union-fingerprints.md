# fix(checker): align discriminated union TS2339 fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/checker-discriminated-union-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the quick-picked fingerprint-only conformance failure
`TypeScript/tests/cases/conformance/types/union/discriminatedUnionTypes2.ts`.
TSZ currently emits the correct diagnostic codes (`TS2339`, `TS2353`) but the
wrong TS2339 fingerprints: it misses tsc's generic-literal union diagnostic for
`x.b` in `f14` and instead emits an extra `Property 'value' does not exist on
type 'never'` diagnostic in the unreachable `foo1` branch.

## Files Touched

- `crates/tsz-checker/src/**` or `crates/tsz-solver/src/**` (exact owner to be determined from root-cause investigation)
- `crates/tsz-checker/tests/**` or `crates/tsz-solver/tests/**` (unit regression test for the owning invariant)

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib` or `cargo nextest run --package tsz-solver --lib` depending on touched crates
- `./scripts/conformance/conformance.sh run --filter "discriminatedUnionTypes2" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
