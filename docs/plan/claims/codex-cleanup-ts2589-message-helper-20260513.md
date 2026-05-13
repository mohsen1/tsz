# chore(checker-tests): reuse TS2589 diagnostic helper

Branch: `codex/cleanup-ts2589-message-helper-20260513`
PR: pending
Status: claim

## Scope

Reuse the shared `check_source_code_messages` helper directly in
`crates/tsz-checker/tests/ts2589_tests.rs` instead of keeping a local
one-line `get_diagnostics` wrapper.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2589_tests::)' --no-fail-fast`
