# [WIP] fix(emitter): initialize computed auto-accessor storage in key emit

- **Date**: 2026-05-12
- **Branch**: `fix/emit-auto-accessor-computed-weakmap-init-20260512`
- **PR**: TBD
- **Status**: claim
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
- focused emitter tests under `crates/tsz-emitter/tests/`

## Verification

- Planned: `env CARGO_INCREMENTAL=0 ./scripts/emit/run.sh --filter=autoAccessor5 --js-only --verbose --concurrency=1`
