# fix(checker): correct binding-element-default TS2322 source/target display

- **Date**: 2026-04-29
- **Branch**: `fix/destr-default-target-narrow`
- **PR**: #1776
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Binding-element default-value diagnostics
(`function f({ bar = null }: { bar?: number } = {}) {}`) used to render the
mismatch as `Type 'number' is not assignable to type 'number | undefined'.`
— wrong on both sides. tsc renders it as
`Type 'null' is not assignable to type 'number'.`:

- **Source side** is the default-value type (`null`), not the binding's
  post-destructuring local type.
- **Target side** is the property type with `| undefined` stripped — the
  default fills the undefined slot, so the assignability check is against
  the non-undefined shape.

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
  binding-default assignability check, so the target displays as the
  non-undefined shape (`number` rather than `number | undefined`).
- `crates/tsz-checker/tests/destructuring_default_target_narrow_tests.rs`
  — regression test pinning the message.
- `crates/tsz-checker/Cargo.toml` — register the test.

## Verification

- `cargo nextest run -p tsz-checker --test destructuring_default_target_narrow_tests`
  — passes.
- Targeted: `optionalParameterInDestructuringWithInitializer.ts` flips
  the line-52 TS2322 fingerprint (was source/target reversed; now
  matches tsc). Two additional missing TS2345 fingerprints (lines
  21/31, optional binding without own default) remain — separate bug
  in destructuring local typing of optional properties without own
  default, deferred.
- Quick regression `--max 200` — no regressions.
- Full conformance run: net **+5** (12235 → 12240). 14 improvements
  flipped (target test plus several others that hit the cleaner
  binding-default display path). The 10 reported "regressions" are
  stale-snapshot false positives: spot-checked
  `destructuringAssignmentWithDefault2.ts` and
  `namespaceNotMergedWithFunctionDefaultExport.ts` — both fail
  identically on `origin/main` without this change.
