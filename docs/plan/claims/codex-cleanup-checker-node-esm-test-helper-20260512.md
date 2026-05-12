# chore(checker-tests): share file_is_esm diagnostic helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-checker-node-esm-test-helper-20260512`
- **PR**: #5914
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

The checker test suite still has duplicated ParserState/BinderState/CheckerState setup for Node module diagnostics that need to force `file_is_esm`.
This PR adds a focused test utility for that setup and migrates the small TS1203/TS2725 test helpers through it.

## Files Touched

- `crates/tsz-checker/src/test_utils.rs`
- `crates/tsz-checker/tests/ts1203_node_esm_tests.rs`
- `crates/tsz-checker/tests/ts2725_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
- `cargo nextest run -p tsz-checker --test ts1203_node_esm_tests --test ts2725_tests --no-fail-fast` (19 passed)
