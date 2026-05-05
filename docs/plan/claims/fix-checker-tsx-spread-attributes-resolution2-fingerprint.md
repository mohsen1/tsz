# fix(checker): align TSX spread attributes resolution fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-tsx-spread-attributes-resolution2-fingerprint`
- **PR**: #3435
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/conformance/jsx/tsxSpreadAttributesResolution2.tsx`.
Both tsc and tsz emit `TS2322` and `TS2739`, but the diagnostic fingerprints
do not match. The planned scope is to diagnose the exact JSX spread attribute
display or anchoring mismatch and fix it through the existing JSX
assignability/display paths.

## Result

The checker now matches tsc's fingerprints for the fixture:

- Object-literal JSX spreads use the contextual spread type for non-generic
  diagnostic display, preserving `y: "2"` instead of widening to `string`.
- A bad spread followed by an excess explicit attribute suppresses the redundant
  unanchored per-spread TS2322, leaving the synthesized whole-attrs diagnostic
  at the explicit attribute.
- Empty React-style class component attrs with synthetic `children` injection use
  the missing-props TS2739 path and print the bare props alias (`PoisonedProp`).

## Files Touched

- `docs/plan/claims/fix-checker-tsx-spread-attributes-resolution2-fingerprint.md`
- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/props/validation.rs`
- `crates/tsz-checker/src/checkers/jsx/spread.rs`
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo nextest run -j 1 -p tsz-checker --test jsx_component_attribute_tests --test ts2322_jsx_spread_strip_children_injection_tests --no-tests=fail`
- `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo nextest run -j 1 -p tsz-checker jsx_excess_attr_after_spread_preserves_spread_source_order jsx_spread_missing_props_listed_for_each_required_target_property --no-tests=fail`
- `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo nextest run -j 1 -p tsz-checker --test jsx_component_attribute_tests cross_file_react_class_empty_attrs_reports_missing_props_not_whole_target_ts2322 test_contextually_typed_jsx_attribute2_react16_fixture_has_no_ts2322 test_contextually_typed_jsx_attribute2_react16_fixture_has_no_ts7006 --no-tests=fail`
- `./scripts/conformance/conformance.sh run --filter "tsxSpreadAttributesResolution2" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`

## Conformance Impact

- `TypeScript/tests/cases/conformance/jsx/tsxSpreadAttributesResolution2.tsx`: FAIL -> PASS
