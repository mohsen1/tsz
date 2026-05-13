# Call Architecture Diagnostic Assertion Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6361
Branch: `codex/cleanup-next-focused-20260513`
Status: WIP

## Scope

Add small local diagnostic assertion helpers in
`crates/tsz-checker/src/tests/call_architecture_tests.rs` and migrate repeated
filter-by-code / collect-message boilerplate in that file.

This is behavior-preserving test cleanup. It does not change roadmap metrics,
compiler behavior, or implementation direction.

## Verification

Planned:

- `cargo fmt --check`
- focused `cargo nextest` for the affected checker library test module
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking ready.
