# fix(checker): drop hardcoded `Factory<` substring suppression in JSX LMA

- **Date**: 2026-05-05
- **Branch**: `claude/nice-darwin-2uuMO`
- **PR**: TBD
- **Status**: claim
- **Workstream**: anti-hardcoding (CLAUDE.md §25) / JSX LibraryManagedAttributes parity

## Intent

Issue #3227 reports that `JSX.LibraryManagedAttributes` evaluation is
discarded whenever the formatted evaluated props type contains the
substring `Factory<` — a printer-output check, not a structural
condition. The check fires for any user code that names a generic type
`Factory`, producing a spurious `TS2741`/`TS2322` while `tsc` accepts the
same source. CLAUDE.md §25 explicitly forbids this pattern: compiler
decisions cannot be driven by user-chosen identifiers or by regex over
printer output.

The hardcoded check is dead with respect to the existing test suite —
removing it leaves all 173 JSX-component-attribute tests green and does
not regress any other file's tests outside of two pre-existing failures
unrelated to JSX. A pair of new regression tests exercises the same LMA
shape with two different user-chosen alias names (`Factory` and `Box`)
to lock in that the rule is structural rather than name-keyed.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction.rs` — delete the
  `format_type(evaluated).contains("Factory<")` branch in
  `apply_jsx_library_managed_attributes`.
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs` — add two
  regression tests using a shared fixture parameterised by the user's
  generic interface name.

## Verification

- `cargo test -p tsz-checker --test jsx_component_attribute_tests`
  (173 passed, 0 failed; baseline was 171, my two new tests added).
- `cargo test -p tsz-checker --test ts2322_tests` (159 passed).
- `cargo test -p tsz-checker --test ts2353_tests` (36 passed; the only
  failure, `recursive_array_union_excess_property_uses_outer_alias_display`,
  is pre-existing on `main` independent of this change).
- `cargo test -p tsz-checker --test ts2300_tests` (47 passed; the only
  failure, `duplicate_identifier_with_default_lib_symbol_reports_lib_locations`,
  is also pre-existing).
- `cargo test -p tsz-checker --test conformance_issues` (856 passed).
- `cargo test -p tsz-checker --test contextual_typing_tests --test generic_call_inference_tests --test mapped_type_errors_conformance_tests --test conditional_infer_tests --test elaboration_wrapper_init_tests --test spread_rest_tests --test signature_assignability_regression_tests --test type_param_modifier_diagnostics_tests --test ts2344_typeof_merged_tests --test ts2322_jsx_spread_strip_children_injection_tests --test jsx_excess_attr_with_spread_source_display_tests --test jsx_import_source_namespace_tests --test jsx_overload_anchor_literal_attr_tests --test jsx_pragma_factory_marks_imports_referenced_tests --test jsx_spread_assignability_suppresses_ts2741`
  (all green).
- `cargo test -p tsz-solver --lib` (5655 passed).
- `cargo fmt --check` (clean).
