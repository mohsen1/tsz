# fix(TS2339): remove annotation-text shortcut that dropped `| undefined` for generic typed identifiers

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-NIJ8c`
- **PR**: TBD
- **Status**: ready
- **Workstream**: fingerprint parity / type display

## Intent

`property_receiver_display_for_node` had a shortcut that, for identifier receivers
with a generic type annotation (e.g. `Required<{ a?: 1; x: 1 }>`), returned the
raw annotation string via `format_annotation_like_type`. That function only does
string-level normalization and never adds `| undefined` for optional properties.
The fix removes this shortcut entirely so the code falls through to
`format_type_for_diagnostic_role → format_property_receiver_type_for_diagnostic`,
which formats through the type system and correctly adds `| undefined` to optional
properties. This resolves the fingerprint mismatch for TS2339 in
`requiredMappedTypeModifierTrumpsVariance`.

## Files Touched

- `crates/tsz-checker/src/error_reporter/properties.rs` (−12 LOC)
- `crates/tsz-checker/src/error_reporter/render_request_tests.rs` (+28 LOC, regression test)

## Verification

- `cargo test -p tsz-checker --lib ts2339_generic_mapped_type_receiver_includes_optional_undefined` passes
- `./scripts/conformance/conformance.sh run --filter requiredMappedTypeModifierTrumpsVariance` — TS2339 fingerprints now match (`missing-fingerprints: []`); one pre-existing TS2322 false positive remains (contextual typing for generic interface applications, separate issue)
