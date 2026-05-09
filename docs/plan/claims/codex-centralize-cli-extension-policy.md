# Centralize CLI file-extension policy

- **Date**: 2026-05-04
- **Branch**: `codex/centralize-cli-extension-policy`
- **PR**: #2697
- **Status**: ready
- **Workstream**: Architecture cleanup

## Intent

Remove duplicated TypeScript/JavaScript/JSON extension-family policy from
`crates/tsz-cli/src/project/fs.rs` and route project discovery, module-file
classification, include glob generation, and declaration/source shadowing
through `tsz-common::file_extensions`.

## Files Touched

- `crates/tsz-common/src/file_extensions.rs`
- `crates/tsz-cli/src/project/fs.rs`

## Verification

- `cargo test -p tsz-common file_extensions -- --nocapture`
- `cargo test -p tsz-cli project::fs -- --nocapture`
- `cargo fmt --check`
- Pre-commit hook: clippy, wasm32 warnings, architecture guardrails, nextest precommit profile
