# JSDoc Template Class Diagnostic Code Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6309
Branch: `codex/cleanup-jsdoc-template-class-codes-20260513`
Status: Ready

## Scope

Deduplicate repeated diagnostic-code projection expressions in
`crates/tsz-checker/tests/jsdoc_template_class_tests.rs`.

This is a behavior-preserving checker-test cleanup. It does not change roadmap
metrics, compiler behavior, or implementation direction.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(jsdoc_template_class_tests::)' --no-fail-fast`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking the PR ready
