#[test]
fn recursive_heritage_conflict_check_does_not_compare_rendered_types() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/checkers/generic_checker/recursive_heritage_constraint.rs"
    ))
    .expect("recursive heritage checker source should be readable");

    let start = source
        .find("pub(super) fn member_has_conflicting_constraint_property")
        .expect("recursive heritage conflict helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n    }\n}")
        .expect("recursive heritage conflict helper should end before impl close");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("format_type_diagnostic"),
        "recursive heritage conflict detection must use structural facts, not rendered type strings"
    );
    assert!(
        helper_body.contains("recursive_heritage_property_types_conflict"),
        "recursive heritage conflict detection should route through the assignability boundary"
    );
}

#[test]
fn call_parameter_array_display_normalization_is_not_gated_by_rendered_text() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/types/computation/call_result.rs"
    ))
    .expect("call result source should be readable");

    let start = source
        .find("fn error_argument_not_assignable_preserving_param_display")
        .expect("call argument diagnostic helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n    fn finite_mapped_parameter_display_type")
        .expect("call argument diagnostic helper should end before next helper");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("target_display.contains(\"Array<\")"),
        "call argument target display normalization must not branch on rendered Array<T> text"
    );
    assert!(
        helper_body.contains("Self::normalize_array_generic_to_shorthand(&target_display)"),
        "call argument target display should always route through the idempotent display normalizer"
    );
}

#[test]
fn mapped_target_type_parameter_containment_is_structural() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/assignability/assignability_diagnostics/argument_reports.rs"
    ))
    .expect("assignability diagnostics source should be readable");

    let start = source
        .find("pub(crate) fn should_suppress_self_referential_mapped_constraint_arg_mismatch")
        .expect("self-referential mapped constraint helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n    fn self_referential_mapped_intersection_accepts_object_literal")
        .expect("self-referential mapped constraint helper should end before next helper");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("format_type_for_assignability_message"),
        "mapped target type-parameter containment must not inspect rendered target text"
    );
    assert!(
        !helper_body.contains(".contains(name.as_ref())"),
        "mapped target type-parameter containment must not string-match user-chosen parameter names"
    );
    assert!(
        helper_body.contains("contains_type_parameter_named("),
        "mapped target type-parameter containment should route through the structural query boundary"
    );
}

#[test]
fn polymorphic_this_intersection_display_is_structural() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/assignability/assignment_checker/assignment_ops.rs"
    ))
    .expect("assignment ops source should be readable");

    let start = source
        .find("fn check_polymorphic_this_property_assignment")
        .expect("polymorphic this assignment helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n    /// Check if an expression produces a `this`-typed value.")
        .expect("polymorphic this assignment helper should end before expression helper");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("source_display.contains(\" & \")"),
        "polymorphic this source display must not branch on rendered intersection text"
    );
    assert!(
        helper_body.contains(
            "query_boundaries::diagnostics::simple_intersection_head_for_this_assignment_display"
        ),
        "polymorphic this source display should route through the structural diagnostic query"
    );

    let diagnostics = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/query_boundaries/diagnostics.rs"
    ))
    .expect("diagnostic query boundaries source should be readable");
    assert!(
        diagnostics.contains("pub(crate) fn simple_intersection_head_for_this_assignment_display"),
        "polymorphic this structural display helper should live in the diagnostic query boundary"
    );
}

#[test]
fn global_object_special_diagnostic_is_structural() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/error_reporter/render_failure.rs"
    ))
    .expect("render failure source should be readable");

    let start = source
        .find("if depth == 0")
        .expect("top-level render failure special cases should exist");
    let body = &source[start..];
    let end = body
        .find("\n        let rctx = RenderContext")
        .expect("top-level render failure special cases should end before render context");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("format_type_diagnostic(source)"),
        "global Object special diagnostic must not prove Object identity through rendered text"
    );
    assert!(
        helper_body.contains("is_global_object_interface_for_diagnostic"),
        "global Object special diagnostic should route through the diagnostic query boundary"
    );
}

#[test]
fn indexed_access_ts2339_suppression_is_structural() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/types/type_checking/indexed_access.rs"
    ))
    .expect("indexed access checker source should be readable");

    assert!(
        !source.contains("type_str_for_check.contains('[')"),
        "indexed-access TS2339 suppression must not infer T[K] from rendered text"
    );
    assert!(
        source.contains("contains_index_access_type("),
        "indexed-access TS2339 suppression should route through the structural query boundary"
    );
}

#[test]
fn mixin_anonymous_class_display_rewrite_is_idempotent() {
    let heritage_source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/classes/class_abstract_checker.rs"
    ))
    .expect("class abstract checker source should be readable");
    assert!(
        !heritage_source.contains("base_str.contains(\"(Anonymous class)\")"),
        "mixin heritage display should apply the anonymous-class rewrite idempotently"
    );

    let constructor_source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/classes/constructor_checker.rs"
    ))
    .expect("constructor checker source should be readable");
    assert!(
        !constructor_source.contains("name.contains(\"(Anonymous class)\")"),
        "mixin constructor display should apply the anonymous-class rewrite idempotently"
    );
}

#[test]
fn ts2411_type_query_constructor_display_uses_symbol_identity() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/state/state_checking_members/index_signature_key_helpers.rs"
    ))
    .expect("index signature key helper source should be readable");
    assert!(
        !source.contains("value_display == symbol.escaped_name"),
        "TS2411 type-query constructor display should use symbol identity, not formatted-name equality"
    );
    assert!(
        source.contains("resolve_type_to_symbol_id(value_type) == Some(sym_id)"),
        "TS2411 type-query constructor display should prove the queried value by symbol identity"
    );
}

#[test]
fn jsx_children_strip_display_uses_type_surface_predicate() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/checkers/jsx/props/validation.rs"
    ))
    .expect("JSX props validation source should be readable");
    assert!(
        !source.contains("stripped_display.starts_with('{')"),
        "JSX stripped-children display should use type surface facts, not rendered object-literal text"
    );
    assert!(
        source.contains("type_has_displayable_name("),
        "JSX stripped-children display should preserve named surfaces structurally"
    );
}

#[test]
fn jsx_children_display_append_is_property_resolution_driven() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/checkers/jsx/diagnostics.rs"
    ))
    .expect("JSX diagnostics source should be readable");
    assert!(
        !source.contains("children_display.is_empty()"),
        "JSX children display append should be driven by property resolution, not formatted text emptiness"
    );
}

#[test]
fn jsx_union_props_class_target_display_is_not_a_decision_gate() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/checkers/jsx/diagnostics.rs"
    ))
    .expect("JSX diagnostics source should be readable");
    assert!(
        !source.contains("class_target.is_empty()"),
        "JSX union props target display should rely on resolved IntrinsicClassAttributes, not formatted text emptiness"
    );
}

#[test]
fn jsx_props_intersection_member_display_is_not_a_decision_gate() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/checkers/jsx/diagnostics.rs"
    ))
    .expect("JSX diagnostics source should be readable");
    assert!(
        !source.contains("formatted.is_empty()"),
        "JSX props intersection display should rely on syntax member presence, not formatted text emptiness"
    );
}

#[test]
fn jsx_library_managed_infer_display_uses_structural_surface() {
    let diagnostics = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/checkers/jsx/diagnostics.rs"
    ))
    .expect("JSX diagnostics source should be readable");
    assert!(
        !diagnostics.contains("raw_display.contains(\"propTypes: infer\")"),
        "JSX LibraryManagedAttributes display should not detect infer metadata from rendered text"
    );

    let boundary = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/query_boundaries/checkers/jsx.rs"
    ))
    .expect("JSX query boundary source should be readable");
    assert!(
        boundary.contains("library_managed_attributes_infer_surface"),
        "JSX LibraryManagedAttributes infer detection should live in the query boundary"
    );
}

#[test]
fn preferred_constructor_display_uses_structural_identity() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/error_reporter/core/diagnostic_source.rs"
    ))
    .expect("diagnostic source should be readable");
    assert!(
        !source.contains("source_display == constructor_display"),
        "preferred constructor display should compare type structure, not rendered text"
    );
    assert!(
        source.contains("are_types_structurally_identical("),
        "preferred constructor display should prove constructor equivalence structurally"
    );
}

#[test]
fn ts2820_target_display_uses_structural_surface_predicates() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/error_reporter/core_formatting.rs"
    ))
    .expect("core formatting source should be readable");
    let start = source
        .find("pub(in crate::error_reporter) fn format_ts2820_target_display")
        .expect("TS2820 target display helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n    pub(super) fn first_nonpublic_constructor_param_property")
        .expect("TS2820 target display helper should end before next helper");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("expanded_target_str == target_str"),
        "TS2820 target display should not decide from equality of rendered target text"
    );
    assert!(
        helper_body.contains("ts2820_target_contains_application_surface(target)")
            && helper_body.contains("ts2820_target_contains_alias_surface(target)"),
        "TS2820 target display should preserve named surfaces through structural predicates"
    );
}

#[test]
fn index_access_type_parameter_ts2719_uses_declared_param_names() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/error_reporter/render_failure.rs"
    ))
    .expect("render failure source should be readable");
    let start = source
        .find("fn render_index_access_type_parameter_mismatch")
        .expect("indexed-access parameter mismatch renderer should exist");
    let body = &source[start..];
    let end = body
        .find("\n    /// Render the TS2322 + TS2517")
        .expect("indexed-access parameter mismatch renderer should end before TS2517 renderer");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("source_param_str == target_param_str"),
        "indexed-access TS2719 elaboration should not compare rendered type-parameter text"
    );
    assert!(
        helper_body.contains("distinct_type_parameters_share_declared_name("),
        "indexed-access TS2719 elaboration should compare type-parameter identity and declared names"
    );
}

#[test]
fn iterator_result_return_mismatch_uses_structural_return_surface() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/assignability/assignability_diagnostics.rs"
    ))
    .expect("assignability diagnostics source should be readable");
    let start = source
        .find("fn iterator_result_return_display_mismatch")
        .expect("iterator-result return mismatch helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n}\n\nfn parse_simple_type_application_display")
        .expect("iterator-result return mismatch helper block should end before display parser");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("format_type(") && !helper_body.contains("function_return_display("),
        "IteratorResult return mismatch should inspect callable return/object shape, not rendered type text"
    );
    assert!(
        helper_body.contains("iterator_result_application_args")
            && helper_body.contains("iterator_result_return_source_has_broad_done"),
        "IteratorResult return mismatch should route through structural IteratorResult and source-return helpers"
    );
}

#[test]
fn missing_property_nominal_requalification_avoids_bare_rendered_name_comparison() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/error_reporter/render_failure_missing_property.rs"
    ))
    .expect("missing-property renderer source should be readable");
    let forbidden = [
        "let fmt_src_bare = self.format_type_diagnostic(widened_source);",
        "let fmt_tgt_bare = self.format_type_diagnostic(target);",
        "fmt_src_bare == fmt_tgt_bare",
    ];
    let violations = forbidden
        .iter()
        .filter(|pattern| source.contains(**pattern))
        .copied()
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "missing-property requalification must use nominal TypeId facts, not \
         re-rendered bare type-name comparison. Violations:\n  {}",
        violations.join("\n  ")
    );
    assert!(
        source.contains("distinct_types_share_nominal_diagnostic_name("),
        "missing-property requalification should route through the diagnostic query boundary"
    );
}

#[test]
fn mapped_declared_source_display_uses_finite_property_surface() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/error_reporter/core/diagnostic_source.rs"
    ))
    .expect("diagnostic source should be readable");

    source
        .find("finite_mapped_property_surface(")
        .expect("mapped declared source display should use finite mapped surface helper");

    assert!(
        !source.contains("declared_structural_display.starts_with('{')")
            && !source.contains("declared_structural_display.contains(\" in \")"),
        "mapped declared source display should not branch on rendered object-literal text"
    );
    assert!(
        source.contains("query_boundaries::diagnostics::finite_mapped_property_surface"),
        "mapped declared source display should route through the diagnostic query boundary"
    );

    let diagnostics = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/query_boundaries/diagnostics.rs"
    ))
    .expect("diagnostics query boundary source should be readable");
    assert!(
        diagnostics.contains("pub(crate) fn finite_mapped_property_surface"),
        "finite mapped display classification should live in the diagnostic query boundary"
    );
}

#[test]
fn generic_constraint_lib_resolution_uses_structural_names() {
    let sources = [
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/checkers/generic_checker/mod.rs"
        ))
        .expect("generic checker source should be readable"),
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/checkers/generic_checker/constraint_validation.rs"
        ))
        .expect("generic constraint validation source should be readable"),
    ];

    for source in sources {
        assert!(
            !source.contains("let constraint_name = self.format_type_diagnostic(constraint);"),
            "generic constraint lib fallback must not derive a semantic decision from rendered constraint text"
        );
        assert!(
            !source.contains("is_well_known_lib_type_name(&constraint_name)"),
            "generic constraint lib fallback should prove lib identity structurally"
        );
    }

    let generic_checker = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/checkers/generic_checker/mod.rs"
    ))
    .expect("generic checker source should be readable");
    assert!(
        generic_checker.contains("resolve_well_known_lib_constraint_type")
            && generic_checker.contains("query_common::lazy_def_id")
            && generic_checker.contains("query_common::get_application_lazy_def_id"),
        "generic constraint lib fallback should inspect Lazy/Application DefIds"
    );
}
