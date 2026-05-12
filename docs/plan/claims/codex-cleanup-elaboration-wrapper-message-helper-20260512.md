# chore(checker-tests): reuse message helper in elaboration wrapper tests

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-elaboration-wrapper-message-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: tbd
- **Status**: claim
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove the local `(code, message_text)` projection in elaboration wrapper
regression tests while keeping the position-aware helper used by span-specific
assertions.

## Scope

- Migrate `crates/tsz-checker/tests/elaboration_wrapper_init_tests.rs` to
  call the existing checker diagnostic message helper for message-only tests.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test elaboration_wrapper_init_tests --no-fail-fast`
