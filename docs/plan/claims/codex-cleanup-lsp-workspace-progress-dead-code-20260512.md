# chore(lsp): remove unused workspace progress state

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-lsp-workspace-progress-dead-code-20260512`
- **PR**: #5684
- **Status**: ready
- **Workstream**: 8.4 (dead code cleanup)

## Intent

Remove dead state from the standalone `tsz_lsp` binary. Workspace folder names
were parsed and stored but never read, and the progress-report helper was never
called; the server only emits begin/end progress notifications today.

## Files Touched

- `crates/tsz-cli/src/bin/tsz_lsp.rs`
- `docs/plan/claims/codex-cleanup-lsp-workspace-progress-dead-code-20260512.md`

## Verification

- `cargo fmt -p tsz-cli`
- `cargo check -p tsz-cli --bin tsz-lsp`
- `cargo clippy -p tsz-cli --bin tsz-lsp -- -D warnings`
- `git diff --check`
