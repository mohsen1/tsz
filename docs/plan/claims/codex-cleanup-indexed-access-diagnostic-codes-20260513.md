# Indexed Access Diagnostic Code Assertion Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6295
Branch: `codex/cleanup-indexed-access-diagnostic-codes-20260513`
Status: WIP

## Scope

Use the shared `diagnostic_codes` checker-test helper for repeated
diagnostic-code projections in indexed-access-oriented checker tests.

Target files:

- `crates/tsz-checker/src/tests/noUIA_any_index_emits_ts2322_tests.rs`
- `crates/tsz-checker/src/tests/noUIA_write_index_signature_emits_ts2322_tests.rs`
- Adjacent checker test files only if they share the same local projection
  pattern and keep the cleanup cohesive.

This is behavior-preserving test cleanup. It does not change roadmap metrics,
compiler behavior, or implementation direction.

## Verification

Planned:

- `cargo fmt --check`
- focused `cargo nextest` for the affected checker test modules
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking ready.
