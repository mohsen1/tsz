# fix(checker): preserve JSDoc generic display name; anchor TS2744 at offending identifier

- **Date**: 2026-04-26
- **Branch**: `fix/jsdoc-generic-type-display-name`
- **PR**: #1450
- **Status**: ready
- **Workstream**: Conformance — fingerprint parity

## Intent

Two fingerprint-only mismatches in `subclassThisTypeAssignable01.ts`:

1. JSDoc `@type {ClassComponent<any>}` lost its type arguments in
   diagnostics ("Type 'C' is not assignable to type 'ClassComponent'.").
   The typedef path already registers a display def
   `Name<Args>` after instantiation; the non-typedef path
   (`resolve_jsdoc_generic_type`) didn't. Extracted a small helper
   `register_jsdoc_generic_display_name` and called it from both paths.

2. TS2744 "type parameter defaults can only reference previously declared
   type parameters" anchored at the start of the entire default-type
   expression (col 64 for `Lifecycle<Attrs, State>`). tsc anchors at the
   offending identifier itself (col 81 for the second `State`).
   `collect_type_references_in_type` already returns the offending
   `NodeIndex`; switched the emission to use it.

## Files Touched

- `crates/tsz-checker/src/jsdoc/resolution/type_construction.rs` (+24/-7 LOC)
- `crates/tsz-checker/src/types/type_checking/core.rs` (+5/-3 LOC)
- `crates/tsz-checker/src/tests/dispatch_tests.rs` (regression test)
- `crates/tsz-checker/tests/generic_tests.rs` (regression test)

## Verification

- `cargo nextest run -p tsz-checker --lib` (2907/2908 pass; 1 pre-existing
  unrelated LOC failure in `error_reporter/core/diagnostic_source.rs`)
- `./scripts/conformance/conformance.sh run --filter "subclassThisTypeAssignable01" --verbose`
  (1/1 PASS, was 0/1 fingerprint-only)
- `./scripts/conformance/conformance.sh run --filter "typeParameter"`
  (98/101 PASS; no new failures)
- `./scripts/conformance/conformance.sh run --filter "default"` (42/43, no new failures)
