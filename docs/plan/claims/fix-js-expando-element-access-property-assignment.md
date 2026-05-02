# fix(checker): infer JS expando element-access assignments

- **Date**: 2026-05-02
- **Branch**: `fix/js-expando-element-access-property-assignment`
- **PR**: #2237
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Checked JavaScript should infer expando object shapes from simple string-literal element-access assignments, matching tsc's `typeFromPropertyAssignment39.ts` behavior. tsz currently records dot-property expando writes but misses assignments such as `foo["baz"] = {}`, so a later nested read like `foo["baz"]["blah"]` emits TS7053. This PR extends the existing expando key extraction path to treat literal bracket keys the same as dot keys when the chain is statically nameable.

## Files Touched

- `crates/tsz-checker/src/types/property_access_helpers/expando.rs`
- `crates/tsz-checker/src/types/computation/access.rs`
- `crates/tsz-checker/src/symbols/name_text.rs`
- `crates/tsz-checker/tests/js_constructor_property_tests.rs`

## Verification

- Passed: `cargo nextest run -p tsz-checker test_js_object_expando_element_access_literal_keys_infer_nested_shape test_js_chained_this_element_assignment_reports_ts7053`
- Passed: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter typeFromPropertyAssignment39 --verbose`
- Passed: `cargo nextest run -p tsz-checker`
- Passed: `cargo nextest run -p tsz-cli`
