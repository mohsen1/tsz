# Call Architecture Diagnostic Assertion Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6361
Branch: `codex/cleanup-next-focused-20260513`
Status: Ready for review

## Scope

Add small local diagnostic assertion helpers in
`crates/tsz-checker/src/tests/call_architecture_tests.rs` and migrate repeated
filter-by-code / collect-message boilerplate in that file.

This is behavior-preserving test cleanup. It does not change roadmap metrics,
compiler behavior, or implementation direction.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(call_architecture_tests::)' --no-fail-fast` (73 passed)
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- `cargo nextest run --workspace --no-fail-fast` (27,101 passed, 35 unrelated failures outside the edited test module)
- Full PR CI after marking ready.
