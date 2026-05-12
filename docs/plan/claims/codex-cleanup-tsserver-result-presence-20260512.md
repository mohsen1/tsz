# chore(cli-tests): tidy optional vector length assertions

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-tsserver-result-presence-20260512`
- **PR**: #5857
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Replace repeated optional-vector length assertion patterns in CLI tests with a small local helper so the tests read by behavior instead of Option plumbing.

## Files Touched

- `crates/tsz-cli/tests/args_tests.rs`
- `crates/tsz-cli/tests/config_tests.rs`

## Verification

- `cargo nextest run -p tsz-cli args_tests config_tests --no-fail-fast`
- `cargo clippy -p tsz-cli --tests -- -D warnings`
