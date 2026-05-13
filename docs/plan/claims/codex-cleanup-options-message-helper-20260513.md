# Claim: add option-aware diagnostic message helper

Status: ready for review
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6192
Branch: `codex/cleanup-options-message-helper-20260513`

## Scope

- Add a shared checker test helper for projecting custom-option diagnostics
  to `(code, message_text)` pairs.
- Replace the local parse/bind/check diagnostic collection helper in
  `crates/tsz-checker/tests/ts2683_tests.rs` with that shared helper.
- Keep the change behavior-preserving and limited to checker test harness
  cleanup.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2683_tests::)|test(test_utils::tests::)' --no-fail-fast`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2683_tests::)|test(test_utils::tests::)' --no-fail-fast`
