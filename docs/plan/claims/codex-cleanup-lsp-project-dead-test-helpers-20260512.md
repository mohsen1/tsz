# chore(lsp): remove dead project test helpers

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-lsp-project-dead-test-helpers-20260512`
- **PR**: #5711
- **Status**: ready
- **Workstream**: 8.4 (dead code cleanup)

## Intent

Remove unused test-only `len`/`is_empty` helpers from the LSP project file
ID allocator and skeleton fingerprint cache. The methods had no call sites
and existed only behind `#[cfg(test)]` plus `#[allow(dead_code)]`, so deleting
them removes stale lint suppressions without changing runtime behavior.

## Files Touched

- `crates/tsz-lsp/src/project/core.rs`
- `docs/plan/claims/codex-cleanup-lsp-project-dead-test-helpers-20260512.md`

## Verification

- `cargo fmt -p tsz-lsp`
- `cargo test -p tsz-lsp project_tests::`
- `cargo clippy -p tsz-lsp --all-targets -- -D warnings`
- `git diff --check`
