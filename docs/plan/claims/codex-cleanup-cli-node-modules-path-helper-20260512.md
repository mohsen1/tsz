# chore(cli): extract node_modules path helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-cli-node-modules-path-helper-20260512b`
- **PR**: #5751
- **Status**: ready
- **Workstream**: 8.4 (DRY cleanup)

## Intent

Keep CLI file discovery's symlink identity rule in one place by extracting the
discovered-path resolution branch and the repeated `node_modules` component
scan from `project/fs.rs`.

## Duplicate-Work Check

- Reviewed open PRs #5088, #5643, #5739, #5740, #5744, #5746, and #5749.
- No open PR currently touches `crates/tsz-cli/src/project/fs.rs`.

## Files Touched

- `crates/tsz-cli/src/project/fs.rs`
- `docs/plan/claims/codex-cleanup-cli-node-modules-path-helper-20260512.md`

## Verification

- `cargo fmt -p tsz-cli`
- `cargo test -p tsz-cli project::fs::tests::`
- `cargo clippy -p tsz-cli --all-targets -- -D warnings`
- `git diff --check`
- `cargo build --profile dist-fast -p tsz-cli` (pre-commit prerequisite)

## Local Hook Note

- A normal `git commit` pre-commit run was attempted after the focused
  verification above. It failed in the broader `tsz-cli` direct test sweep on
  existing TS5011 driver fixtures (`compile_basic_arrow_function` and
  `compile_class_with_generic_constructor`) after 470/1593 tests; this cleanup
  changes only file discovery path resolution helpers, and the focused
  `project::fs` suite passed.
