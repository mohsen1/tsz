# [WIP] fix(checker): match merge symbol reexport function diagnostics

- **Date**: 2026-05-04
- **Branch**: `fix/merge-symbol-reexport-function`
- **PR**: #2720
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

Fix the randomly picked conformance failure
`TypeScript/tests/cases/compiler/mergeSymbolRexportFunction.ts`. The expected
tsc fingerprint is TS2451, while tsz currently emits TS1362, TS2300, and
TS2349. This PR classifies function-valued type-only reexports correctly for
targeted module-augmentation collisions and resolves the augmentation value
surface for consumers.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/duplicate_identifier_conflict_kinds.rs`
- `crates/tsz-checker/src/types/type_checking/duplicate_identifiers.rs`
- `crates/tsz-checker/src/types/type_checking/commonjs_object_exports.rs`
- `crates/tsz-checker/src/types/type_checking/cross_file_conflicts.rs`
- `crates/tsz-checker/src/types/module_augmentation.rs`
- `crates/tsz-checker/src/types/computation/identifier/core.rs`
- `crates/tsz-checker/tests/ts2451_cross_file_augmentation_tests.rs`

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-checker --test ts2451_cross_file_augmentation_tests`
- `./scripts/conformance/conformance.sh run --filter "mergeSymbolRexportFunction" --verbose` => `1/1 passed`
- `./scripts/conformance/conformance.sh run --max 200` => `200/200 passed`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` => `12423/12582 passed`, net `+3`, no regressions
