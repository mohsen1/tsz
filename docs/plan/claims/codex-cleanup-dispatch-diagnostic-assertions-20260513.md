# Dispatch Diagnostic Assertion Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6379
Branch: `codex/cleanup-dispatch-diagnostic-assertions-20260513`
Status: Ready for review

## Scope

Add small local diagnostic assertion helpers in
`crates/tsz-checker/src/tests/dispatch_tests.rs` and migrate repeated
filter-by-code / collect-code / collect-message boilerplate in that file.

This is behavior-preserving test cleanup. It does not change roadmap metrics,
compiler behavior, or implementation direction.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(dispatch_tests::)' --no-fail-fast` (122 passed)
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking ready.
