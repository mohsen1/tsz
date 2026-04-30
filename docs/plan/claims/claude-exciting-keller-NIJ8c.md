# fix(TS2339): skip annotation-text shortcut for inline object type literals

- **Date**: 2026-04-30
- **Branch**: `claude/exciting-keller-NIJ8c`
- **PR**: TBD
- **Status**: ready
- **Workstream**: fingerprint parity / type display

## Intent

`property_receiver_display_for_node` had a shortcut that, for identifier receivers
with a type annotation starting with a letter, returned the raw annotation string
via `format_annotation_like_type` when the type was a generic application or had a
display alias. That function only does string-level normalization and never adds
`| undefined` for optional properties.

When the annotation contains an inline object literal (e.g. `Required<{ a?: 1; x: 1 }>`),
the raw annotation text omits `| undefined`, producing a fingerprint mismatch:

- tsz: `Property 'b' does not exist on type 'Required<{ a?: 1; x: 1 }>'`
- tsc: `Property 'b' does not exist on type 'Required<{ a?: 1 | undefined; x: 1; }>'`

The fix adds `&& !annotation.contains('{')` to the shortcut guard. When the annotation
contains `{`, the code falls through to `format_type_for_diagnostic_role(PropertyReceiver)`
which routes through `format_property_receiver_type_for_diagnostic` — a proper
type-system formatter that calls `format_property` with
`preserve_optional_property_surface_syntax=false`, correctly adding `| undefined`.

The shortcut is still applied for simple annotations without inline object types
(e.g. `bar: Bar<Foo>`, `bar: Bar`), preserving the alias-name display behavior.

## Files Touched

- `crates/tsz-checker/src/error_reporter/properties.rs` (+2 LOC guard, +3 LOC comment)
- `crates/tsz-checker/src/error_reporter/render_request_tests.rs` (+36 LOC regression test)
- `docs/plan/claims/claude-exciting-keller-NIJ8c.md` (this file)

## Verification

- `cargo test -p tsz-checker --lib -- render_request_tests::ts2339_generic_mapped_type_receiver_includes_optional_undefined` passes
- `./scripts/conformance/conformance.sh run --filter requiredMappedTypeModifierTrumpsVariance` — TS2339 fingerprints now match (`missing-fingerprints: []`); one pre-existing TS2322 false positive for `Foo<{ a?: 1; x: 1 }>` contextual typing remains unrelated to this fix
