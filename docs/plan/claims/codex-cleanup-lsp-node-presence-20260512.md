# codex/cleanup-lsp-node-presence-20260512

Status: ready
Owner: codex
Created: 2026-05-12 00:21:40 UTC

## Intent

Simplify LSP document-symbol sentinel `NodeIndex` presence checks from
`!idx.is_none()` to the idiomatic `idx.is_some()` form.

## Scope

- `crates/tsz-lsp/src/symbols/document_symbols.rs`
- This claim file

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-lsp`
- Pre-commit checks as appropriate for the focused LSP-only cleanup
