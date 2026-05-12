# chore(wasm): remove stale transform context allowances

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-wasm-transform-context-allowances-20260512`
- **PR**: #5697
- **Status**: ready
- **Workstream**: 8.4 (dead code cleanup)

## Intent

Remove stale `#[allow(dead_code)]` attributes from `WasmTransformContext`
fields that are read by `emit_with_transforms`. The attributes no longer
describe the current code and hide useful compiler feedback.

## Files Touched

- `crates/tsz-core/src/api/wasm/transforms.rs`
- `docs/plan/claims/codex-cleanup-wasm-transform-context-allowances-20260512.md`

## Verification

- `cargo fmt -p tsz-core`
- `cargo check -p tsz-core`
- `cargo clippy -p tsz-core --all-targets -- -D warnings`
- `cargo test -p tsz-core transform_api_tests`
- `cargo check -p tsz-core --target wasm32-unknown-unknown`
