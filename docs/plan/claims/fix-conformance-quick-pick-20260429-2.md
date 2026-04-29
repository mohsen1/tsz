# fix(checker): suppress extra contextual unknown symbol diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-2`
- **PR**: #1789
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the quick-picked conformance failure `unknownSymbolOffContextualType1.ts`.
TSZ currently emits the expected TS2339 plus extra TS2403 and TS2551 diagnostics.
This PR suppresses only the invalid extra diagnostics while preserving the
expected missing-property error.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-checker/src/types/property_access_augmentation.rs`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter unknownSymbolOffContextualType1 --verbose`
