# Centralize lib capability policy

- **Date**: 2026-05-04
- **Branch**: `codex/lib-capability-policy-table`
- **PR**: #2701
- **Status**: ready
- **Workstream**: Architecture cleanup

## Intent

Move binder/checker lib-symbol policy into a shared capability table so global
validation, ES lib type suggestions, and value-position lib suggestions stop
maintaining independent raw lists. Keep DOM-only globals, such as `console`,
separate from baseline ES global validation.

## Files Touched

- `crates/tsz-common/src/lib_capabilities.rs`
- `crates/tsz-common/src/lib.rs`
- `crates/tsz-binder/src/lib_loader.rs`
- `crates/tsz-binder/src/binding/validation.rs`

## Verification

- `CARGO_INCREMENTAL=0 cargo test -p tsz-common lib_capabilities -- --nocapture`
- `CARGO_INCREMENTAL=0 cargo test -p tsz-binder lib_loader -- --nocapture`
- `CARGO_INCREMENTAL=0 cargo test -p tsz-binder validate_global_symbols -- --nocapture`
- `CARGO_INCREMENTAL=0 cargo test -p tsz-checker --lib environment_capabilities -- --nocapture`
- `CARGO_INCREMENTAL=0 cargo test -p tsz-checker --test missing_global_type_diagnostics_tests -- --nocapture`
- `cargo fmt --check`
