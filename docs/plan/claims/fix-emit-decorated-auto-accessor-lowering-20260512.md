# fix(emitter): lower decorated public auto-accessors

- **Date**: 2026-05-12
- **Branch**: `fix/emit-decorated-auto-accessor-lowering-20260512`
- **PR**: #6042
- **Status**: ready
- **Workstream**: 2 (JS Emit And Declaration Emit Pass Rate)

## Intent

Fix JS emit parity for decorated public auto-accessors that currently survive as
raw `accessor` class elements instead of TypeScript's storage/getter/setter
lowering. The first focused target is `staticAutoAccessorsWithDecorators`, which
is failing for ES2017 and ES2022 output.

## Files Touched

- `crates/tsz-emitter/src/transforms/es_decorators.rs`
- `crates/tsz-emitter/src/transforms/helpers.rs`
- `crates/tsz-emitter/src/emitter/declarations/class/helpers.rs`
- `crates/tsz-emitter/src/emitter/source_file/emit.rs`

## Verification

- `cargo fmt`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-emitter`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-emitter transforms::helpers::tests::emit_helpers_order_decorators_and_async_helpers`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- `env CARGO_INCREMENTAL=0 TSZ_BIN=/Users/dutchess/Documents/tsz/.target/release/tsz ./scripts/emit/run.sh --filter=staticAutoAccessorsWithDecorators --js-only --verbose --concurrency=1`
- `env CARGO_INCREMENTAL=0 TSZ_BIN=/Users/dutchess/Documents/tsz/.target/release/tsz ./scripts/emit/run.sh --filter=esDecorators-classDeclaration-missingEmitHelpers-staticComputedAutoAccessor --js-only --verbose --concurrency=1`
- `env CARGO_INCREMENTAL=0 TSZ_BIN=/Users/dutchess/Documents/tsz/.target/release/tsz ./scripts/emit/run.sh --filter=esDecorators-classDeclaration-missingEmitHelpers-nonStaticPrivateAutoAccessor --js-only --verbose --concurrency=1`
- `env CARGO_INCREMENTAL=0 TSZ_BIN=/Users/dutchess/Documents/tsz/.target/release/tsz ./scripts/emit/run.sh --filter=esDecorators-classDeclaration-missingEmitHelpers-staticPrivateAutoAccessor --js-only --verbose --concurrency=1`
- Broader check: `env CARGO_INCREMENTAL=0 TSZ_BIN=/Users/dutchess/Documents/tsz/.target/release/tsz ./scripts/emit/run.sh --filter=autoAccessor --js-only --verbose --concurrency=1` (47/47 pass)
