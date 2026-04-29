# [WIP] fix(solver): avoid false TS2741 in recursive conditional BuildTree

- **Date**: 2026-04-29
- **Branch**: `fix/recursive-conditional-buildtree-ts2741`
- **PR**: #1709
- **Status**: claim
- **Workstream**: Conformance (Workstream 1)

## Intent

Fix the remaining conformance divergence in
`excessPropertyCheckIntersectionWithRecursiveType.ts`, where TSZ reports an
extra TS2741 and misses a TS2339 after recursive conditional type evaluation.
Prior PR #1374 fixed mixed fixed+rest tuple inference for `Prepend`; this slice
continues from its documented follow-up around recursive conditional
instantiation / fuel behavior.

## Files Touched

- `crates/tsz-solver/src/**` (expected; exact files after diagnosis)
- `crates/tsz-checker/tests/**` or `crates/tsz-solver/tests/**` (unit
  regression test)

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib`
- `cargo nextest run --package tsz-solver --lib`
- `./scripts/conformance/conformance.sh run --filter "excessPropertyCheckIntersectionWithRecursiveType" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
