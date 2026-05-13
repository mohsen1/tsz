# chore(checker-tests): reuse TS7036 diagnostic helper

Branch: `codex/cleanup-ts7036-message-helper-20260513`
Status: ready for review

## Scope

Reuse the shared `check_source_code_messages` helper directly in
`crates/tsz-checker/tests/ts7036_tests.rs` instead of keeping a local
one-line `get_diagnostics` wrapper.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts7036_tests::)' --no-fail-fast`

## Verification

- `cargo fmt --check` passed.
- `cargo nextest run -p tsz-checker --lib -E 'test(ts7036_tests::)' --no-fail-fast` passed: 8 tests.
