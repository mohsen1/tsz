# Claim: reuse TS7041 diagnostic message helper

Status: WIP
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6160
Branch: `codex/cleanup-ts7041-message-helper-20260513`

## Scope

- Replace the local parse/bind/check diagnostic collection helper in
  `crates/tsz-checker/tests/ts7041_tests.rs` with the shared
  `check_source_code_messages` helper.
- Keep the change behavior-preserving and limited to checker test harness
  cleanup.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts7041_tests::)' --no-fail-fast`
