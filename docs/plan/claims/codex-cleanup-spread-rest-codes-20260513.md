# Spread/Rest Diagnostic Code Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6321
Branch: `codex/cleanup-spread-rest-diagnostic-codes-20260513`
Status: Implemented

## Scope

Deduplicate repeated diagnostic-code projection expressions in
`crates/tsz-checker/tests/spread_rest_tests.rs`.

This is a behavior-preserving checker-test cleanup. It does not change roadmap
metrics, compiler behavior, or implementation direction.

## Verification

Completed:

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test spread_rest_tests --no-fail-fast`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
