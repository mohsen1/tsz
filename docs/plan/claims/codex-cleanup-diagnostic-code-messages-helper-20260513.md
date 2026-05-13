# Shared Checker-Test Diagnostic Code/Message Helper

Issue: https://github.com/mohsen1/tsz/issues/6349
Branch: `codex/cleanup-diagnostic-code-messages-helper-20260513`
Status: WIP

## Scope

Add a shared diagnostic `(code, message_text)` projection helper to
`crates/tsz-checker/src/test_utils.rs` and migrate a larger cluster of
checker display tests that currently hand-roll the same projection locally.

This is behavior-preserving test cleanup. It does not change roadmap metrics,
compiler behavior, or implementation direction.

## Verification

Planned:

- `cargo fmt --check`
- focused `cargo nextest` runs for touched checker test targets
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking ready.
