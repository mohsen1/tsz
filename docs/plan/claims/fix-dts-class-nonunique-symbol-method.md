# [WIP] fix(emitter): preserve symbol indexer method declarations

- **Date**: 2026-05-02
- **Branch**: `fix/dts-class-nonunique-symbol-method`
- **PR**: #2209
- **Status**: ready
- **Workstream**: 2 (declaration emit pass rate)

## Intent

Investigate and fix the declaration emit mismatch for
`classNonUniqueSymbolMethodHasSymbolIndexer`. The target is a narrow emitter
change that preserves TypeScript-compatible declaration output for classes
using symbol indexers and non-unique symbol methods without broad printer
string heuristics.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- Focused emit repro for `classNonUniqueSymbolMethodHasSymbolIndexer`.
- `cargo fmt --check`
- `cargo nextest run -p tsz-cli declaration_emit_generic_call_preserves_class_expression_type_argument`
- `cargo nextest run -p tsz-cli declaration_emit_imported_generic_call_preserves_function_type_argument`
- `cargo nextest run -p tsz-emitter` (1710 passed, 5 skipped)
- `TSZ_BIN=/tmp/tsz-tail-failures/.target/release/tsz scripts/safe-run.sh ./scripts/emit/run.sh --dts-only --filter=classNonUniqueSymbolMethodHasSymbolIndexer --verbose --json-out=/tmp/tsz-tail-failures/.tmp-class-symbol-indexer-final2.json`
