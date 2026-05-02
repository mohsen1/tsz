# fix(checker): resolve import-equals merged namespace values

- Date: 2026-05-02
- Branch: `fix/dts-import-equals-enum-reference`
- PR: #2225
- Status: validated locally
- Workstream: 2 (conformance/declaration emit)

## Intent

Fix `declarationEmitEnumReferenceViaImportEquals`, where `import X = Namespace.X`
used as a property-access receiver resolved to the type-alias side of a merged
namespace export instead of the value-side const object.

## Files Touched

- `crates/tsz-checker/src/types/computation/identifier/core.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-cli declaration_emit_import_equals_alias_to_merged_namespace_value_allows_property_access`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter declarationEmitEnumReferenceViaImportEquals --verbose`
- `cargo nextest run -p tsz-checker`
