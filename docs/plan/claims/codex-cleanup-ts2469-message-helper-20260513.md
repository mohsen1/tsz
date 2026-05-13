# Claim: reuse TS2469 diagnostic message helper

Status: WIP
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6204
Branch: `codex/cleanup-ts2469-message-helper-20260513`

## Scope

- Replace the local parse/bind/check diagnostic collection helper in
  `crates/tsz-checker/tests/ts2469_symbol_operator_tests.rs` with the shared
  `check_source_code_messages` helper.
- Keep the existing common declaration prefix for the symbol operator samples.
- Keep the change behavior-preserving and limited to checker test harness
  cleanup.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2469_symbol_operator_tests::)' --no-fail-fast`
