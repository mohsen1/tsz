# [WIP] fix(emitter): lower decorated public auto-accessors

- **Date**: 2026-05-12
- **Branch**: `fix/emit-decorated-auto-accessor-lowering-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (JS Emit And Declaration Emit Pass Rate)

## Intent

Fix JS emit parity for decorated public auto-accessors that currently survive as
raw `accessor` class elements instead of TypeScript's storage/getter/setter
lowering. The first focused target is `staticAutoAccessorsWithDecorators`, which
is failing for ES2017 and ES2022 output.

## Files Touched

- `crates/tsz-emitter/src/transforms/es_decorators.rs`
- related class/auto-accessor emit helpers if required by the decorator transform

## Verification

- Planned: `env CARGO_INCREMENTAL=0 TSZ_BIN=/Users/dutchess/Documents/tsz/.target/release/tsz ./scripts/emit/run.sh --filter=staticAutoAccessorsWithDecorators --js-only --verbose --concurrency=1`
