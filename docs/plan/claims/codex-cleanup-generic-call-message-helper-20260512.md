# chore(checker-tests): inline generic call diagnostic message helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-generic-call-message-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: tbd
- **Status**: claim
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a trivial local passthrough wrapper in generic-call primitive widening
display tests. The file already delegates to the shared checker diagnostic
message helper, so call that helper directly.

## Scope

- Migrate `crates/tsz-checker/tests/generic_call_primitive_widening_display_tests.rs`.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test generic_call_primitive_widening_display_tests --no-fail-fast`
