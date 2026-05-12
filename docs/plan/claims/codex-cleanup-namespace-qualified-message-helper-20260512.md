# chore(checker-tests): inline namespace-qualified diagnostic helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-namespace-qualified-message-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: tbd
- **Status**: claim
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a trivial local passthrough diagnostic wrapper in namespace-qualified
diagnostic display tests by importing the existing checker diagnostic message
helper directly.

## Scope

- Migrate `crates/tsz-checker/tests/namespace_qualified_diagnostic_tests.rs`.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test namespace_qualified_diagnostic_tests --no-fail-fast`
