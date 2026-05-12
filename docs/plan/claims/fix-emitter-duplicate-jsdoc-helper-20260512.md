# fix(emitter): remove duplicate jsdoc helper

- **Date**: 2026-05-12
- **Branch**: `fix/emitter-duplicate-jsdoc-helper-20260512`
- **PR**: #5733
- **Status**: ready
- **Workstream**: 8.4 (repo hygiene)

## Intent

Restore workspace compilation after adjacent emitter JSDoc helper changes landed
with two identical `is_simple_jsdoc_type_name` methods in
`declaration_emitter/helpers/jsdoc.rs`.

## Duplicate-Work Check

- Reviewed current open PR changed files.
- No open PR currently touches
  `crates/tsz-emitter/src/declaration_emitter/helpers/jsdoc.rs`.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/jsdoc.rs`
- `docs/plan/claims/fix-emitter-duplicate-jsdoc-helper-20260512.md`

## Verification

- `cargo check -p tsz-emitter` passed locally.
