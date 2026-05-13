# Claim: reuse TS2450 option-aware diagnostic helper

Status: WIP
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6200
Branch: `codex/cleanup-ts2450-options-helper-20260513`

## Scope

- Replace the local option-aware parse/bind/check diagnostic collection helper
  in `crates/tsz-checker/tests/ts2450_const_enum_tests.rs` with the shared
  `check_with_options_code_messages` helper.
- Keep the change behavior-preserving and limited to checker test harness
  cleanup.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2450_const_enum_tests::)' --no-fail-fast`
