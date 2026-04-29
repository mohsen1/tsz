# fix(checker): drop typeof-import alias for direct-only CJS exports

- **Date**: 2026-04-29
- **Branch**: `fix/cjs-direct-export-no-namespace-tag`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Files whose entire export surface is a single `module.exports = { … }`
object literal (no `exports.foo = …` augmentation, no
`Object.defineProperty(exports, …)`, no prototype members) display as the
literal shape — `{ a: number; }` — in tsc diagnostics. tsz was tagging the
synthesized type with `namespace_module_names` unconditionally, so
require-side diagnostics rendered `typeof import("mod")` instead of the
literal shape, breaking fingerprint parity.

The synth-pipeline now distinguishes "named exports came from real
augmentation" vs "named exports were just the seed of the direct-export
literal", and only inserts the typeof-import display tag when the surface
is actually namespace-like.

## Files Touched

- `crates/tsz-checker/src/query_boundaries/js_exports.rs`:
  - new `JsExportSurface::has_augmented_named_exports` field.
  - `compute_js_export_surface` sets it when `augment_namespace_props_…`
    contributes any property beyond the seed.
  - `to_type_id_with_display_name` and `js_export_surface_namespace_type`
    only insert `namespace_module_names` when the synth is namespace-like
    (no direct export, or has augmentation, or has prototype members).
- `crates/tsz-checker/src/state/type_analysis/computed_commonjs/exports_detection.rs`:
  initialize the new field at the inline construction site.
- `crates/tsz-checker/tests/cjs_direct_object_export_no_typeof_import_display_tests.rs`:
  regression test asserting `{ a: number; }` is the displayed shape.
- `crates/tsz-checker/Cargo.toml`: register the new test target.

## Verification

- `cargo nextest run -p tsz-checker --lib` — 2964/2964 pass.
- `cargo nextest run -p tsz-checker --test cjs_direct_object_export_no_typeof_import_display_tests`
  — passes.
- `./scripts/conformance/conformance.sh run --filter
  "requireOfJsonFileInJsFile.ts"` — 1/1 (was 0/1).
- Full conformance run pending before flipping `Status: ready`.
