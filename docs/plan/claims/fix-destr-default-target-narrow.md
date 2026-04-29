# fix(checker): correct binding-element-default TS2322 source/target display

- **Date**: 2026-04-29
- **Branch**: `fix/destr-default-target-narrow-redo`
- **PR**: #1790 (replaces closed #1776)
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Binding-element default-value diagnostics
(`function f({ bar = null }: { bar?: number } = {}) {}`) used to render
the mismatch as
`Type 'number' is not assignable to type 'number | undefined'.` — wrong
on both sides. tsc renders it as
`Type 'null' is not assignable to type 'number'.`:

- **Source side** is the default-value type (`null`), not the binding's
  post-destructuring local type.
- **Target side** is the property type with `| undefined` stripped — the
  default fills the undefined slot, so the assignability check is against
  the non-undefined shape.

This is a redo of the closed PR #1776 on a fresh branch off current
`origin/main` so the merge-base is clean.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
  — guard `direct_diagnostic_source_expression` to return `None` when the
  anchor is the `name` of a `BINDING_ELEMENT`. Mirrors the existing
  `VARIABLE_DECLARATION.name` guard.
- `crates/tsz-checker/src/error_reporter/core/type_display.rs`
  — handle `BINDING_ELEMENT` in `assignment_source_expression`'s parent
  walk: prefer the binding's own initializer, falling through to the
  enclosing parameter only when no own default is present.
- `crates/tsz-checker/src/types/type_checking/core.rs`
  — narrow `element_type` via `narrow_destructuring_default` before the
  binding-default assignability check so the target displays as the
  non-undefined shape.
- `crates/tsz-checker/tests/destructuring_default_target_narrow_tests.rs`
  — regression test pinning the message.
- `crates/tsz-checker/Cargo.toml` — register the test.

## Verification

- `cargo nextest run -p tsz-checker --test destructuring_default_target_narrow_tests`
  — passes.
- Minimal-repro `.ts` file emits the expected `Type 'null' is not assignable
  to type 'number'.` diagnostic.
- `cargo fmt --check`
- `cargo check --package tsz-checker`
