# Claim: reuse union target diagnostic message helper

Status: WIP
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6209
Branch: `codex/cleanup-union-target-message-helper-20260513`

## Scope

- Replace the local parse/bind/check diagnostic collection helper in
  `crates/tsz-checker/tests/union_target_literal_primitive_mismatch_tests.rs`
  with the shared option-aware `check_with_options_code_messages` helper.
- Keep the strict/null-check option behavior unchanged.
- Keep the change behavior-preserving and limited to checker test harness
  cleanup.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(union_target_literal_primitive_mismatch_tests::)' --no-fail-fast`
