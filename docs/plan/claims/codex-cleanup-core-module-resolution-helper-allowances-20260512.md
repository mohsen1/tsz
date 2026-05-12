# chore(core): remove stale module resolution helper allowances

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-core-module-resolution-helper-allowances-20260512`
- **PR**: #5706
- **Status**: ready
- **Workstream**: 8.4 (dead code cleanup)

## Intent

Remove stale `#[allow(dead_code)]` attributes from the module resolution
test helper and its private `TS2792` constant. Both are used by
`module_resolution_tests`, so the attributes hide no real warning and add
unnecessary lint noise.

## Files Touched

- `crates/tsz-core/tests/module_resolution_tests.rs`
- `docs/plan/claims/codex-cleanup-core-module-resolution-helper-allowances-20260512.md`

## Verification

- `cargo fmt -p tsz-core`
- `cargo test -p tsz-core module_resolution_tests::`
- `cargo clippy -p tsz-core --all-targets -- -D warnings`
