# Name Resolution Diagnostic Assertion Cleanup

Issue: https://github.com/mohsen1/tsz/issues/6414
Branch: `codex/cleanup-name-resolution-diagnostic-assertions-20260513`
Status: Ready for review

## Scope

Clean up repeated diagnostic-code count, presence, and filter assertions in
`crates/tsz-checker/tests/name_resolution_boundary_tests.rs`.

The module repeatedly hand-rolls diagnostic-code iterator projections across
the name-resolution boundary diagnostic families. This is behavior-preserving
test cleanup.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test name_resolution_boundary_tests --no-fail-fast` (46 tests passed)
- `cargo clippy --profile ci-lint -p tsz-checker --all-targets -- -D warnings`
- Full PR CI after marking ready.
