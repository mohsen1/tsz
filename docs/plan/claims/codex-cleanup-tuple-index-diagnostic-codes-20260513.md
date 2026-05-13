# Tuple Index Diagnostic Code Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6290
Branch: `codex/cleanup-tuple-index-diagnostic-codes-20260513`
Status: WIP

## Scope

Deduplicate repeated diagnostic-code projection expressions in
`crates/tsz-checker/tests/tuple_index_access_tests.rs`.

This is a behavior-preserving checker-test cleanup. It does not change roadmap
metrics, compiler behavior, or implementation direction.

## Verification

Planned:

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(tuple_index_access_tests::)' --no-fail-fast`
- Full PR CI after marking the PR ready
