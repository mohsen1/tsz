//! Ratchet tests for the `query_boundaries::common` migration quarantine.
//!
//! `query_boundaries/common.rs` is the historical default import for any
//! checker code needing a semantic type fact. Domain modules (`diagnostics`,
//! `type_predicates`, `key_constraints`, `assignability`, `state::checking`,
//! ...) now own slices of that surface, but nothing structurally prevents a
//! new domain-specific helper from landing back in `common.rs` during a parity
//! fix. Without a ratchet the quarantine refills faster than the domain slices
//! drain it (see issue #12948, parent #8225 / #8223).
//!
//! These tests pin the exact set of `pub(crate) fn` definitions allowed to live
//! in `common.rs`. Adding *or* removing a definition forces an explicit edit to
//! [`ALLOWED_COMMON_PUB_CRATE_FNS`], which surfaces in review. Helpers that have
//! already been migrated to a domain owner are additionally guarded against
//! reappearing in `common.rs`.
//!
//! ## Regenerating the allowlist
//!
//! After an intentional change to the `common.rs` function surface, regenerate
//! the sorted list with:
//!
//! ```bash
//! grep -oE '^\s*pub\(crate\) fn [a-z_0-9]+' \
//!     crates/tsz-checker/src/query_boundaries/common.rs \
//!     | sed -E 's/.*fn //' | sort -u | awk '{printf "    \"%s\",\n", $0}'
//! ```
//!
//! and paste the result into [`ALLOWED_COMMON_PUB_CRATE_FNS`]. A removal is the
//! desired direction (the quarantine draining); an addition should be reviewed
//! against the structural rule that a domain-specific semantic policy helper
//! belongs in its named domain boundary module, not `common.rs`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const COMMON_PATH: &str = "src/query_boundaries/common.rs";
const DIAGNOSTICS_PATH: &str = "src/query_boundaries/diagnostics.rs";

/// Resolve a checker-crate-relative path against the manifest directory so
/// the scan works regardless of the test runner's working directory.
fn checker_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// Snapshot of every `pub(crate) fn` defined in `common.rs`. Sorted and unique.
/// Regenerate with the command documented in the module header.
const ALLOWED_COMMON_PUB_CRATE_FNS: &[&str] = &[
    "application_id",
    "application_info",
    "apply_contextual_type",
    "array_applicable_type",
    "array_element_type",
    "call_signatures_for_type",
    "callable_shape_for_type",
    "callable_shape_for_type_extended",
    "callable_shape_id",
    "classify_for_augmentation",
    "classify_for_contextual_literal",
    "classify_for_evaluation",
    "classify_for_interface_merge",
    "classify_for_literal_value",
    "classify_for_predicate_signature",
    "classify_for_traversal",
    "classify_for_type_resolution",
    "classify_identity_mapped",
    "classify_literal_type",
    "classify_namespace_member",
    "classify_type_query",
    "collect_callable_property_types",
    "construct_return_type_for_type",
    "create_string_literal_type",
    "enum_components",
    "enum_def_id",
    "enum_member_type",
    "evaluate_type",
    "extract_contextual_type_params",
    "find_matching_property",
    "find_property_by_str",
    "find_property_in_object",
    "find_property_in_object_by_str",
    "format_excess_property_name",
    "free_type_params_named",
    "function_shape_for_type",
    "function_shape_id",
    "get_application_base",
    "get_application_lazy_def_id",
    "get_base_constraint_of_type",
    "get_base_type_for_comparison",
    "get_call_signatures",
    "get_callable_shape_for_type",
    "get_conditional_type_id",
    "get_construct_return_type_union",
    "get_construct_signatures",
    "get_fixed_tuple_length",
    "get_indexed_access_type",
    "get_invalid_index_type_member",
    "get_iterator_info",
    "get_merged_object_shape_for_type",
    "get_noinfer_inner",
    "get_private_brand_name",
    "get_private_field_name",
    "get_readonly_inner",
    "get_tuple_element_type_union",
    "get_type_query_symbol_ref",
    "homomorphic_mapped_source",
    "index_access_parts",
    "index_access_types",
    "instantiate_function_with_type_args",
    "instantiate_type_preserving_meta",
    "instantiate_type_with_depth_status",
    "intersect_constructor_returns",
    "intersection_list_id",
    "intersection_members",
    "intersection_or_single",
    "keyof_inner_type",
    "keyof_object_properties",
    "lazy_def_id",
    "lazy_resolve_failure_count",
    "literal_value",
    "map_compound_members_if_changed",
    "mapped_property_type",
    "mapped_type_id",
    "mapped_type_info",
    "needs_evaluation_for_merge",
    "new_binary_op_evaluator",
    "no_infer_inner_type",
    "normalize_object_union_members_for_write_target",
    "number_literal_value",
    "object_shape_for_type",
    "object_shape_id",
    "object_symbol",
    "object_with_index_shape_id",
    "raw_property_type",
    "readonly_inner_type",
    "remove_nullish",
    "remove_undefined",
    "resolve_default_type_args",
    "resolve_unbound_type_params_to_declared_fallbacks",
    "resolve_unbound_type_params_to_defaults",
    "rest_argument_element_type",
    "return_type_for_type",
    "should_preserve_application_for_inference",
    "split_nullish_type",
    "string_intrinsic_components",
    "string_literal_value",
    "stringify_literal_type",
    "substitute_this_type",
    "substitute_this_type_at_return_position",
    "tuple_elements",
    "tuple_leading_fixed_drill_cap",
    "tuple_list_id",
    "type_application",
    "type_is_conditional_type_result_with_unresolved_inference",
    "type_param_info",
    "type_parameter_constraint",
    "type_parameter_default",
    "type_query_symbol",
    "type_shape_symbol",
    "types_are_comparable_for_assertion",
    "union_list_id",
    "union_members",
    "union_with_undefined",
    "unique_symbol_ref",
    "unpack_tuple_rest_parameter",
    "unwrap_readonly",
    "unwrap_readonly_or_noinfer",
    "widen_freshness",
    "widen_literal_to_primitive",
    "widen_literal_type",
    "widen_type",
    "widen_type_deep",
    "widen_type_for_display",
];

/// Display-widening helpers whose definition now lives in `diagnostics`
/// (the #12948 migration slice).
const DISPLAY_WIDENING_IN_DIAGNOSTICS: &[&str] = &[
    "deep_reduce_for_display",
    "normalize_display_property_order",
    "widen_argument_type_for_display",
    "boolean_literal_array_display_type",
    "get_base_constraint_for_display",
    "display_widen_for_redeclaration",
];

/// Display / compiler-managed predicates migrated to `diagnostics` and
/// `type_predicates` by #12916.
const MIGRATED_OUT_OF_COMMON_12916: &[&str] = &[
    "is_compiler_managed_type",
    "indexed_access_alias_body",
    "is_unresolved_for_display",
    "type_may_display_iterator_protocol",
    "function_signature_has_typeof",
];

/// `FunctionShape` instantiation, parameter-list transformation, and
/// redeclaration-widening helpers migrated to `generic_instantiation`,
/// `signature_building`, and `widening` by the #15643 arch-health paydown
/// (parent #8225).
const MIGRATED_OUT_OF_COMMON_15643: &[&str] = &[
    "instantiate_function_shape",
    "instantiate_shape_to_defaults",
    "sanitize_params_at_positions",
    "params_to_tuple_elements",
    "sanitize_callable_shape_binding_pattern_params",
    "widen_function_literal_return_type",
    "widen_callable_literal_return_types",
];

/// Structural shape predicates and containment/traversal queries migrated to
/// `shape_predicates` and `containment_queries` by the second #8225 arch-health
/// paydown slice (the `common.rs` 1762/1764 headroom exhaustion). Six wrappers
/// that had no remaining callers (`collect_enum_def_ids`, `contains_lazy_def_id`,
/// `contains_type_parameter_named_shallow`, `has_unresolved_type_parameters`,
/// `is_infer_type`, `is_primitive_or_literal_compound`) were deleted outright and
/// must not return to `common.rs` either.
const MIGRATED_OUT_OF_COMMON_SHAPE_CONTAINMENT: &[&str] = &[
    "are_same_base_literal_kind",
    "collect_all_types",
    "collect_enum_def_ids",
    "collect_lazy_def_ids",
    "collect_referenced_types",
    "collect_type_queries",
    "constraint_references_type_param_in_resolution_path",
    "contains_application_in_structure",
    "contains_conditional_type",
    "contains_current_infer_placeholder",
    "contains_error_type",
    "contains_error_type_in_args",
    "contains_file_relative_content",
    "contains_free_type_parameters",
    "contains_generic_indexed_access_surface",
    "contains_generic_type_parameters",
    "contains_index_access_type",
    "contains_infer_types",
    "contains_keyof_type",
    "contains_lazy_def_id",
    "contains_lazy_or_recursive",
    "contains_never_type",
    "contains_this_type",
    "contains_type_by_id",
    "contains_type_parameter_named",
    "contains_type_parameter_named_shallow",
    "contains_type_parameters",
    "has_call_signatures",
    "has_construct_signatures",
    "has_deferred_conditional_member",
    "has_function_shape",
    "has_late_bound_members",
    "has_nonpublic_property",
    "has_property_by_str",
    "has_unresolved_type_parameters",
    "is_array_or_tuple_type",
    "is_array_type",
    "is_bare_infer_placeholder",
    "is_bigint_type",
    "is_boolean_type",
    "is_callable_type",
    "is_conditional_type",
    "is_constructor_like_type",
    "is_deferred_indexed_access_or_intersection_with_one",
    "is_definitely_nullish",
    "is_distributive_conditional_with_deferred_check",
    "is_empty_object_type",
    "is_enum_type",
    "is_error_type",
    "is_evaluable_meta_type",
    "is_fresh_object_type",
    "is_function_type",
    "is_generic_application",
    "is_generic_application_with_type_params",
    "is_generic_mapped_application",
    "is_generic_mapped_type",
    "is_generic_type",
    "is_genuine_error_type",
    "is_homomorphic_mapped_type_context",
    "is_index_access_type",
    "is_infer_type",
    "is_intersection_type",
    "is_keyof_type",
    "is_lazy_type",
    "is_literal_or_primitive_or_compound_of_those",
    "is_literal_type",
    "is_literal_type_through_type_constraints",
    "is_mapped_type",
    "is_mapped_type_with_readonly_modifier",
    "is_merged_intersection_object",
    "is_module_namespace_type",
    "is_nullish_type",
    "is_number_literal",
    "is_number_type",
    "is_object_like_type",
    "is_object_or_mapped_type",
    "is_only_null_or_undefined",
    "is_plain_object_type",
    "is_primitive_or_literal_compound",
    "is_primitive_type",
    "is_readonly_tuple_fixed_element",
    "is_spread_marker_tuple",
    "is_string_intrinsic_type",
    "is_string_literal",
    "is_string_type",
    "is_structurally_deferred_type",
    "is_symbol_or_unique_symbol",
    "is_symbol_type",
    "is_template_literal_type",
    "is_this_type",
    "is_tuple_like_type",
    "is_tuple_type",
    "is_type_deeply_any",
    "is_type_parameter",
    "is_type_parameter_like",
    "is_type_parameter_or_intersection_with_type_parameter",
    "is_type_query_type",
    "is_union_type",
    "is_unique_symbol_type",
    "is_unit_type",
    "is_unresolved_inference_result",
    "is_valid_mapped_type_key_type",
    "is_widening_primitive_intrinsic",
    "mapped_type_is_deferred_generic",
    "numeric_literal_index_valid_for_object",
    "references_any_type_param_named",
    "return_type_is_unresolved",
    "type_contains_string_literal",
    "type_contains_undefined",
    "type_has_displayable_name",
    "type_has_readonly_members",
    "type_id_is_known_to_db",
    "type_parameter_has_conditional_constraint",
    "type_parameter_has_mapped_constraint",
    "union_contains_tuple",
    "union_of_bare_lazy_def_ids",
    "walk_referenced_types",
];

/// All helpers that have been migrated out of `common.rs` to a named domain
/// owner and must not reappear as a `common.rs` definition. Derived from the
/// per-campaign slices so the two lists cannot drift apart.
fn migrated_out_of_common() -> Vec<&'static str> {
    MIGRATED_OUT_OF_COMMON_12916
        .iter()
        .chain(DISPLAY_WIDENING_IN_DIAGNOSTICS)
        .chain(MIGRATED_OUT_OF_COMMON_15643)
        .chain(MIGRATED_OUT_OF_COMMON_SHAPE_CONTAINMENT)
        .copied()
        .collect()
}

/// Whether `source` defines a free function named `name`, in either the plain
/// `fn name(` or generic `fn name<` form. Matches the bare-`fn` convention used
/// by the sibling architecture-contract tests.
fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}(")) || source.contains(&format!("fn {name}<"))
}

/// Extract the names of every `pub(crate) fn` defined at the top level of a
/// source file (handles both plain and generic `fn name<...>` forms).
fn extract_pub_crate_fns(source: &str) -> BTreeSet<String> {
    const MARKER: &str = "pub(crate) fn ";
    let mut names = BTreeSet::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(MARKER) else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            names.insert(name);
        }
    }
    names
}

#[test]
fn common_pub_crate_fn_surface_matches_allowlist() {
    let source = fs::read_to_string(checker_path(COMMON_PATH))
        .expect("failed to read query_boundaries/common.rs");
    let actual = extract_pub_crate_fns(&source);
    let allowed: BTreeSet<String> = ALLOWED_COMMON_PUB_CRATE_FNS
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    let added: Vec<&String> = actual.difference(&allowed).collect();
    let removed: Vec<&String> = allowed.difference(&actual).collect();

    assert!(
        added.is_empty(),
        "new pub(crate) fn definitions landed in query_boundaries/common.rs without review: {added:?}. \
         A domain-specific semantic policy helper belongs in its named domain boundary module \
         (diagnostics / type_predicates / key_constraints / assignability / ...), not common.rs. \
         If this is an intentional cross-domain primitive, add it to ALLOWED_COMMON_PUB_CRATE_FNS \
         in tests/arch_source_scans/common_boundary_export_ratchets.rs (see the regeneration \
         command in that file)."
    );
    assert!(
        removed.is_empty(),
        "pub(crate) fn definitions were removed from query_boundaries/common.rs but the ratchet \
         allowlist still lists them: {removed:?}. Regenerate ALLOWED_COMMON_PUB_CRATE_FNS in \
         tests/arch_source_scans/common_boundary_export_ratchets.rs so the quarantine snapshot \
         stays accurate."
    );
}

#[test]
fn migrated_helpers_do_not_reappear_in_common() {
    let source = fs::read_to_string(checker_path(COMMON_PATH))
        .expect("failed to read query_boundaries/common.rs");
    let mut violations = Vec::new();
    for name in migrated_out_of_common() {
        if defines_fn(&source, name) {
            violations.push(name);
        }
    }
    assert!(
        violations.is_empty(),
        "helpers already migrated to a domain boundary reappeared as definitions in \
         query_boundaries/common.rs: {violations:?}. Keep their definition in the owning domain \
         module and route callers there."
    );
}

#[test]
fn display_widening_cluster_is_owned_by_diagnostics() {
    let common = fs::read_to_string(checker_path(COMMON_PATH))
        .expect("failed to read query_boundaries/common.rs");
    let diagnostics = fs::read_to_string(checker_path(DIAGNOSTICS_PATH))
        .expect("failed to read query_boundaries/diagnostics.rs");
    for name in DISPLAY_WIDENING_IN_DIAGNOSTICS {
        assert!(
            defines_fn(&diagnostics, name),
            "query_boundaries::diagnostics must own the display-widening helper {name}"
        );
        assert!(
            !defines_fn(&common, name),
            "display-widening helper {name} must not be defined in common.rs"
        );
    }
}
