# [WIP] fix(checker): suppress extra contextual unknown symbol diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-2`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the quick-picked conformance failure `unknownSymbolOffContextualType1.ts`.
TSZ currently emits the expected TS2339 plus extra TS2403 and TS2551 diagnostics.
This PR will diagnose the root cause in the checker/solver boundary, suppress only
the invalid extra diagnostics, and add a focused regression test in the owning crate.

## Files Touched

- TBD after diagnosis

## Verification

- `./scripts/conformance/conformance.sh run --filter "unknownSymbolOffContextualType1" --verbose`
- targeted owning-crate `cargo nextest run` test
