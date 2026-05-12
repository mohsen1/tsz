# chore(config): share JSONC normalization for project references

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-shared-jsonc-normalizer-20260512`
- **PR**: #5668
- **Status**: ready
- **Workstream**: 8.4 (DRY config cleanup)

## Intent

Remove the project-reference parser's local copy of the tsconfig JSONC
comment/trailing-comma cleanup helpers. `tsz-core::config` already owns this
behavior for normal tsconfig parsing, so expose a shared `normalize_jsonc`
helper and route project-reference parsing through it.

## Files Touched

- `crates/tsz-core/src/config/mod.rs`
- `crates/tsz-cli/src/project/refs.rs`
- `crates/tsz-cli/src/project_refs_tests.rs`
- `docs/plan/claims/codex-cleanup-shared-jsonc-normalizer-20260512.md`

## Verification

- `cargo fmt -p tsz-core -p tsz-cli`
- `cargo check -p tsz-core -p tsz-cli`
- `cargo clippy -p tsz-core -p tsz-cli --lib -- -D warnings`
- `cargo test -p tsz-core --lib config::tests::test_normalize_jsonc_removes_comments_and_trailing_commas -- --exact`
- `cargo test -p tsz-cli --lib project::refs::tests::test_parse_tsconfig_with_references_accepts_jsonc -- --exact`
- `git diff --check`
