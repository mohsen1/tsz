# chore(emitter): remove stale private accessor async flag

- **Date**: 2026-05-12
- **Branch**: `codex/remove-private-accessor-async-field-20260512`
- **PR**: #5656
- **Status**: ready
- **Workstream**: 8.4 (DRY emitter helpers)

## Intent

Remove the unused `PrivateAccessorDef::is_async` field from the emitter. The
field was only written as `false`, never read, and required a local
`#[allow(dead_code)]`; removing it keeps private accessor metadata aligned with
the data the emitter actually consumes.

## Files Touched

- `crates/tsz-emitter/src/emitter/core.rs`
- `crates/tsz-emitter/src/emitter/declarations/class/emit_es6.rs`
- `docs/plan/claims/codex-remove-private-accessor-async-field-20260512.md`

## Verification

- `cargo fmt -p tsz-emitter`
- `cargo check -p tsz-emitter`
- `cargo clippy -p tsz-emitter --all-targets -- -D warnings`
- `git diff --check`
