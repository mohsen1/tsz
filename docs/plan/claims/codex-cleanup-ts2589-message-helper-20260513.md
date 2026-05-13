# chore(checker-tests): reuse TS2589 diagnostic helper

Branch: `codex/cleanup-ts2589-message-helper-20260513`
PR: [#6122](https://github.com/mohsen1/tsz/pull/6122)
Status: ready for review

## Scope

Reuse the shared `check_source_code_messages` helper directly in
`crates/tsz-checker/tests/ts2589_tests.rs` instead of keeping a local
one-line `get_diagnostics` wrapper.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2589_tests::)' --no-fail-fast`

## Verification

- `cargo fmt --check` passed after rebasing onto `origin/main`.
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2589_tests::)' --no-fail-fast` passed: 17 tests.
