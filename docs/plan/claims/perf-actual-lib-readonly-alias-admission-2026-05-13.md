# Claim: Readonly actual-lib alias admission

Date: 2026-05-13
Status: prepared after #6594

## Claim

`Readonly<T>` is a safe first generic actual-lib alias to admit through the
direct alias-body path. The proof/admission split already proves its actual-lib
declarations, `DefinitionStore` body, and type-parameter arity match the
existing child-checker fallback. This slice changes only the explicit admission
policy for `Readonly`; `Record`, `Partial`, `FlatArray`, `IteratorResult`, and
the remaining alias residues stay on fallback.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - adds `Readonly` to `is_direct_actual_lib_alias_body_admitted`.
  - reports `DirectActualLibAliasBodyOutcome::Success` only for admitted names;
    unadmitted generic aliases still report `GenericAlias`.
  - keeps the direct return path gated on a successful proof outcome.
  - preserves cached type parameters for type-alias entries that hit
    `lib_delegation_cache`, while non-alias lib-cache hits keep their existing
    empty-parameter return behavior.
- Unit coverage asserts:
  - `Readonly` resolves through `direct_actual_lib_symbol_type`.
  - the returned type is neither `UNKNOWN` nor `ERROR`.
  - the direct result and `lib_delegation_cache` preserve the single `T` type
    parameter.
  - a later lib-cache hit for `Readonly` also returns the cached alias type
    parameter.
  - `Record` remains on fallback, and the proof/fallback parity test still
    covers `Record`, `Partial`, and `Readonly`.

## Validation

- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type -- --nocapture`
- `cargo test -p tsz-checker --lib direct_actual_lib_alias_proof_matches_mapped_utility_fallback_bodies -- --nocapture`
- `cargo test -p tsz-checker --test generic_alias_assignability_pollution_tests -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests test_ts2322_flatarray_assignment_keeps_rhs_declared_alias_display -- --nocapture`
- `cargo test -p tsz-checker --lib zod_issue_5030_defaults_path_with_lib_utility_aliases -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_module_exports_define_property_does_not_fall_back_to_lib_signature -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_type_literal_bare_uint8array_does_not_poison_later_defaulted_refs -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_typed_array_to_locale_string_uses_options_parameter_type -- --nocapture`
- `cargo test -p tsz-common perf_counters::json_tests -- --nocapture`
- `cargo check -p tsz-checker`
- `cargo fmt --all --check`
- `git diff --check`
- `cargo build -p tsz-cli --release --features perf-tools`
