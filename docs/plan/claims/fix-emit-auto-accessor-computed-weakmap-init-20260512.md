# [WIP] fix(emitter): initialize computed auto-accessor storage in key emit

- **Date**: 2026-05-12
- **Branch**: `fix/emit-auto-accessor-computed-weakmap-init-20260512`
- **PR**: #6032
- **Status**: ready
- **Workstream**: 2 (JS Emit And Declaration Emit Pass Rate)

## Intent

Fix JS emit parity for computed public auto-accessors lowered to WeakMap storage.
`autoAccessor5` currently initializes instance auto-accessor WeakMaps after the class
declaration/IIFE, while TypeScript folds those initializations into the first
computed getter key so constructor-time storage writes cannot observe an
uninitialized WeakMap.

## Files Touched

- `crates/tsz-emitter/src/emitter/declarations/class/emit_es6.rs`
- `crates/tsz-emitter/src/transforms/class_es5_ir.rs`
- `crates/tsz-emitter/src/transforms/class_es5_ir_members.rs`
- `crates/tsz-emitter/src/transforms/ir.rs`

## Verification

- Passed: `env CARGO_INCREMENTAL=0 cargo check -p tsz-emitter`
- Passed: `env CARGO_INCREMENTAL=0 TSZ_BIN=/Users/dutchess/Documents/tsz/.target/release/tsz ./scripts/emit/run.sh --filter=autoAccessor5 --js-only --verbose --concurrency=1`
- Broader slice: `env CARGO_INCREMENTAL=0 TSZ_BIN=/Users/dutchess/Documents/tsz/.target/release/tsz ./scripts/emit/run.sh --filter=autoAccessor --js-only --concurrency=4` still has 5 decorator-related failures outside this claim (`esDecorators-classDeclaration-missingEmitHelpers-*`, `staticAutoAccessorsWithDecorators`).
