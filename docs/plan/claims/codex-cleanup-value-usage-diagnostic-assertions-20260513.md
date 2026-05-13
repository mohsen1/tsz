# Value Usage Diagnostic Assertion Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6402
Branch: `codex/cleanup-value-usage-diagnostic-assertions-20260513`
Status: WIP

## Scope

Clean up repeated diagnostic-code count and presence assertions in
`crates/tsz-checker/tests/value_usage_tests.rs`.

The module repeats direct diagnostic-code iterator projections across
type-as-value, arithmetic operand, namespace/value usage, unknown-value, and
operator diagnostic cases. This is behavior-preserving test cleanup.

## Verification

Planned:

- `cargo fmt --check`
- focused `cargo nextest` for `value_usage_tests`
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking ready.
