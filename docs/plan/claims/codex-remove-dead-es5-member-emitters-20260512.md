# chore(emitter): remove dead ES5 member emitter paths

- **Date**: 2026-05-12
- **Branch**: `codex/remove-dead-es5-member-emitters-20260512`
- **PR**: #5649
- **Status**: ready
- **Workstream**: 8.4 (DRY emitter helpers)

## Intent

Remove obsolete `emit_methods_ir` and `emit_static_members_ir` implementations
that were explicitly superseded by `emit_all_members_ir`. This reduces the ES5
class member emitter's duplicated logic while preserving the single live member
emission path.

## Files Touched

- `crates/tsz-emitter/src/transforms/class_es5_ir_members.rs` (~670 LOC removed)
- `docs/plan/claims/codex-remove-dead-es5-member-emitters-20260512.md`

## Verification

- `cargo fmt -p tsz-emitter`
- `cargo check -p tsz-emitter`
- `cargo clippy -p tsz-emitter --all-targets -- -D warnings`
- `git diff --check`
