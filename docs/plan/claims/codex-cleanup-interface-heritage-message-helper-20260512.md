# chore(checker-tests): inline interface heritage diagnostic message helper

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-interface-heritage-message-helper-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: tbd
- **Status**: claim
- **Labels**: `DRY`, `checker-tests`

## Intent

Remove a trivial local passthrough wrapper in interface heritage display tests.
The file already uses the shared checker diagnostic message helper; this PR
routes call sites to it directly.

## Scope

- Migrate `crates/tsz-checker/tests/interface_heritage_display_tests.rs`
  away from its local `diagnostics` passthrough.

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test interface_heritage_display_tests --no-fail-fast`
