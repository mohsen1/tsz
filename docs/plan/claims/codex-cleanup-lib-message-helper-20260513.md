# Claim: add lib-backed diagnostic message helper

Status: WIP
Owner: Codex
Issue: https://github.com/mohsen1/tsz/issues/6236
Branch: `codex/cleanup-lib-message-helper-20260513`

## Scope

- Add a shared `check_source_with_libs_code_messages` helper in checker test
  utilities.
- Reuse it in `generic_call_inference_tests.rs` for lib-backed diagnostic
  `(code, message_text)` projections.
- Keep this as a behavior-preserving checker test harness cleanup.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(check_source_with_libs_code_messages)'`
- `cargo nextest run -p tsz-checker --test generic_call_inference_tests --no-fail-fast`
