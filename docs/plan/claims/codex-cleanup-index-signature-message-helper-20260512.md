# chore(checker-tests): reuse diagnostic message helper for index signature rewrite tests

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-index-signature-message-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: tbd
- **Status**: claim
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a local `(code, message_text)` diagnostic wrapper in the index-signature
rewrite regression tests by using the existing checker test utility.

## Scope

- Migrate `crates/tsz-checker/tests/source_file_index_signatures_rewrite_tests.rs`
  to `check_source_code_messages`.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test source_file_index_signatures_rewrite_tests --no-fail-fast`
