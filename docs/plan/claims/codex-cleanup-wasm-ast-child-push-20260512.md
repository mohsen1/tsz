# chore(wasm): centralize AST child pushes

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-wasm-ast-child-push-20260512`
- **PR**: #5829
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Refactor the WASM AST child collector to use small local helpers for optional
child nodes and node arrays. The goal is to reduce copy-paste branching in
`get_node_children` while preserving the existing child ordering and public
WASM API surface.

## Files Touched

- `crates/tsz-wasm/src/wasm_api/ast.rs` (~40 LOC cleanup)

## Verification

- `cargo nextest run -p tsz-wasm --no-fail-fast` (46 tests pass)
