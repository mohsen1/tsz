# chore(checker-tests): reuse TS7036 diagnostic helper

Branch: `codex/cleanup-ts7036-message-helper-20260513`
Status: WIP

## Scope

Reuse the shared `check_source_code_messages` helper directly in
`crates/tsz-checker/tests/ts7036_tests.rs` instead of keeping a local
one-line `get_diagnostics` wrapper.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test ts7036_tests --no-fail-fast`
