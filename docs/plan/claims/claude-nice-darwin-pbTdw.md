# [WIP] fix(lsp): return all union/intersection constituents from typeDefinition

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-pbTdw`
- **PR**: TBD
- **Status**: claim
- **Workstream**: LSP correctness — closes issue #4758

## Intent

`textDocument/typeDefinition` on a union (`A | B`) or intersection (`A & B`)
type position currently returns only the location of the first constituent.
The branch in `crates/tsz-lsp/src/navigation/type_definition.rs:286-291`
recurses into `find_type_annotation_child` (which returns just the first
matching child) and short-circuits, with an explicit comment noting the
truncation.

This claim is for fixing that branch to iterate every constituent type child,
recursively resolve each, flatten and dedupe results by `(file_path, range)`,
and return the combined list — preserving declaration order. LSP allows
multiple `TypeDefinitionResult` locations, and tsserver returns all
constituents for the same query.

## Files Touched

- `crates/tsz-lsp/src/navigation/type_definition.rs` — multi-child enumeration helper, multi-result union/intersection branch, dedup.
- `crates/tsz-lsp/tests/type_definition_tests.rs` — strengthen the existing union/intersection test cases to assert both constituents are returned, dedup case.

## Verification

- `cargo nextest run -p tsz-lsp`
- `cargo nextest run -p tsz-checker --lib` (sanity)
