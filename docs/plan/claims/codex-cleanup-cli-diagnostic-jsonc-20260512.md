# chore(cli): reuse shared JSONC normalization for output flags

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-cli-diagnostic-jsonc-20260512`
- **PR**: #5673
- **Status**: ready
- **Workstream**: 8.4 (DRY CLI cleanup)

## Intent

Remove the CLI binary's local JSONC comment-stripping helper for the early
tsconfig output-flag scan. The shared config normalizer now handles comments
and trailing commas, so the CLI should reuse it instead of carrying a smaller
parser variant.

## Files Touched

- `crates/tsz-cli/src/bin/tsz.rs`
- `crates/tsz-cli/tests/tsc_compat_tests.rs`
- `docs/plan/claims/codex-cleanup-cli-diagnostic-jsonc-20260512.md`

## Verification

- `cargo fmt -p tsz-cli`
- `cargo check -p tsz-cli`
- `cargo clippy -p tsz-cli --all-targets -- -D warnings`
- `cargo test -p tsz-cli --test tsc_compat_tests tsconfig_output_only_flags_accept_jsonc_trailing_commas -- --exact`
- `git diff --check`
