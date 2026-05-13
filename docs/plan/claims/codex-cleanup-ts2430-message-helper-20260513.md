# chore(checker-tests): reuse TS2430 diagnostic helper

Branch: `codex/cleanup-ts2430-message-helper-20260513`
PR: [#6117](https://github.com/mohsen1/tsz/pull/6117)
Status: ready

## Scope

Reuse the shared `check_source_code_messages` helper directly in
`crates/tsz-checker/tests/ts2430_tests.rs` instead of keeping a local
one-line `get_diagnostics` wrapper.

## Verification Plan

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2430_tests::)' --no-fail-fast`
- `cargo build -p tsz-cli --bin tsz`
- `cargo nextest run --workspace --lib --bins --no-fail-fast`

Note: the full workspace unit sweep passed 22,418 tests and failed 9 tests
outside this cleanup's touched file/surface.
