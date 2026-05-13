# Shared Checker-Test Diagnostic Code Helper

Issue: https://github.com/mohsen1/tsz/issues/6327
Branch: `codex/cleanup-shared-diagnostic-codes-helper-20260513`
Status: Implemented

## Scope

Add a shared diagnostic-code projection helper to
`crates/tsz-checker/src/test_utils.rs` and replace local/repeated projection
helpers across checker tests.

Touched areas:

- JSDoc diagnostic assertion tests
- Tuple index access tests
- Contextual TS2345 tests
- Spread/rest tests
- Remaining direct TS2322 diagnostic-code projection

This is a behavior-preserving checker-test cleanup. It does not change roadmap
metrics, compiler behavior, or implementation direction.

Related narrower cleanup PRs already covered some per-file dedupe before this
branch became implementation-ready. This branch now keeps the useful shared
helper layer and migrates the remaining focused local helpers/direct projections
without expanding into unrelated checker tests.

## Verification

Run:

- `cargo fmt --check`
- focused `cargo nextest` runs for touched test modules
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking the PR ready
