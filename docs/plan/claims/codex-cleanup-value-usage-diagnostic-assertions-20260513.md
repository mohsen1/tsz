# Value Usage Diagnostic Assertion Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6402
Branch: `codex/cleanup-value-usage-diagnostic-assertions-20260513`
Status: Ready for review

## Scope

Clean up repeated diagnostic-code count and presence assertions in
`crates/tsz-checker/tests/value_usage_tests.rs`.

The module repeats direct diagnostic-code iterator projections across
type-as-value, arithmetic operand, namespace/value usage, unknown-value, and
operator diagnostic cases. This is behavior-preserving test cleanup.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --lib -E 'test(value_usage_tests::)' --no-fail-fast` (39 tests passed)
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking ready.
