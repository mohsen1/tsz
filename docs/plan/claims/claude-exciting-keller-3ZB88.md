# fix(formatter): suppress `| undefined` in optional property types with exactOptionalPropertyTypes

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-3ZB88`
- **PR**: TBD
- **Status**: ready
- **Workstream**: Conformance fingerprint parity (TS2375 display)

## Intent

With `exactOptionalPropertyTypes: true`, `foo?: T` means the property is either
absent or holds `T` — it does NOT implicitly include `undefined`. The
`TypeFormatter` was appending `| undefined` to optional property types in
diagnostic messages even when `exactOptionalPropertyTypes` was active, causing
TS2375 messages to show `{ foo?: number | undefined }` instead of the correct
`{ foo?: number }`. This fix adds `with_exact_optional_property_types(bool)` to
`TypeFormatter` and wires it into the two diagnostic formatter sites that format
types for assignability messages.

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/mod.rs` — new `with_exact_optional_property_types()` builder method
- `crates/tsz-checker/src/context/def_mapping.rs` — wire into `create_diagnostic_type_formatter()`
- `crates/tsz-checker/src/error_reporter/core_formatting.rs` — wire into `format_type_for_assignability_message()`
- `crates/tsz-checker/src/lib.rs` — register new test module
- `crates/tsz-checker/tests/ts2375_exact_optional_property_display_tests.rs` — unit tests
