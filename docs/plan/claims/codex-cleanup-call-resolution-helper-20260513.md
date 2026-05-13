# Claim: reuse call resolution diagnostic helper

Status: Ready
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6223
Branch: `codex/cleanup-call-resolution-helper-20260513`

## Scope

- Replace local custom-option diagnostic projection in
  `crates/tsz-checker/tests/call_resolution_regression_tests.rs` with the
  shared `check_with_options_code_messages` helper.
- Preserve the existing TS2318 missing-global-type filtering.
- Keep the change behavior-preserving and limited to checker test harness
  cleanup.

## Verification Plan

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo nextest run -p tsz-checker --test call_resolution_regression_tests --no-fail-fast`
