//! Contiguous test shard split out of the parent module to satisfy the
//! source-file line cap.

use super::*;

/// Compiler-managed-name checks are semantic predicates and should have a
/// domain query boundary instead of being allowlisted as direct solver access.
#[test]
fn test_compiler_managed_name_predicate_has_domain_boundary() {
    let common_source = fs::read_to_string("src/query_boundaries/common.rs")
        .expect("failed to read query_boundaries/common.rs");
    let predicates_source = fs::read_to_string("src/query_boundaries/type_predicates.rs")
        .expect("failed to read query_boundaries/type_predicates.rs");
    let import_guard_source =
        fs::read_to_string("src/tests/architecture_contract_tests/part_01.rs")
            .expect("failed to read architecture_contract_tests/part_01.rs");

    assert!(
        !common_source.contains("fn is_compiler_managed_type("),
        "is_compiler_managed_type belongs in query_boundaries::type_predicates, not common.rs"
    );
    assert!(
        predicates_source.contains("fn is_compiler_managed_type("),
        "query_boundaries::type_predicates must own is_compiler_managed_type"
    );
    assert!(
        !import_guard_source.contains("\"is_compiler_managed_type\""),
        "is_compiler_managed_type must not be a direct solver import allowlist entry"
    );

    fn walk_rs(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_rs(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs(&src, &mut files);
    let mut violations = Vec::new();
    for file in files {
        let rel = file.strip_prefix(&src).unwrap_or(&file);
        if rel.starts_with("query_boundaries") || rel.starts_with("tests") {
            continue;
        }
        let source = fs::read_to_string(&file).expect("failed to read checker source");
        if source.contains("tsz_solver::is_compiler_managed_type")
            || source.contains("use tsz_solver::is_compiler_managed_type")
        {
            violations.push(rel.display().to_string());
        }
    }
    assert!(
        violations.is_empty(),
        "checker code must call query_boundaries::type_predicates::is_compiler_managed_type; violations: {violations:?}"
    );
}

/// JSX children type-shape queries must route through `query_boundaries::checkers::jsx`,
/// not call `query_boundaries::common` directly from `checkers/jsx/children.rs`.
///
/// The `checkers/jsx/children.rs` module is the only JSX checker file that performs
/// substantial type-shape probing (array-ness, tuple elements, union members, object shape,
/// etc.). All such probes must go through the domain boundary module so that the boundary
/// can evolve without touching call sites.
#[test]
fn test_jsx_children_type_shape_queries_use_domain_boundary() {
    let children_source =
        fs::read_to_string("src/checkers/jsx/children.rs").expect("failed to read children.rs");

    assert!(
        !children_source.contains("query_boundaries::common::"),
        "checkers/jsx/children.rs must not call query_boundaries::common:: directly; \
        route all type-shape probes through query_boundaries::checkers::jsx instead"
    );

    // Verify the domain boundary wrapper module is actually used
    assert!(
        children_source.contains("query_boundaries::checkers::jsx"),
        "checkers/jsx/children.rs must import and use query_boundaries::checkers::jsx"
    );
}

/// Enum arithmetic/widening predicates must live behind the enum-analysis
/// boundary so checker callers do not reopen local enum/member symbol walks.
#[test]
fn test_enum_arithmetic_and_widening_facts_use_enum_analysis_boundary() {
    let enum_checker_source =
        fs::read_to_string("src/checkers/enum_checker.rs").expect("failed to read enum_checker.rs");
    for removed_helper in [
        "fn is_enum_type(",
        "fn is_enum_like_type(",
        "fn is_unresolved_lazy_type(",
        "fn is_enum_or_enum_member(",
        "ENUM_MEMBER",
    ] {
        assert!(
            !enum_checker_source.contains(removed_helper),
            "enum_checker.rs should not own enum semantic helper `{removed_helper}`; use query_boundaries::enum_analysis"
        );
    }

    let boundary_source = fs::read_to_string("src/query_boundaries/enum_analysis.rs")
        .expect("failed to read enum_analysis.rs");
    for helper in [
        "fn is_enum_type(",
        "fn is_arithmetic_enum_like_type(",
        "fn is_unresolved_lazy_type(",
        "fn enum_member_parent_symbol_for_widening(",
        "fn is_enum_member_for_widening(",
    ] {
        assert!(
            boundary_source.contains(helper),
            "query_boundaries::enum_analysis must own `{helper}`"
        );
    }

    for (path, helper) in [
        (
            "src/dispatch/mod.rs",
            "enum_query::is_arithmetic_enum_like_type(",
        ),
        (
            "src/types/computation/helpers.rs",
            "enum_query::is_arithmetic_enum_like_type(",
        ),
        (
            "src/types/computation/binary.rs",
            "enum_query::is_enum_type(",
        ),
        // The literal-freshness boundary (#15390/#15445) now owns the
        // widening *timing*: binding/return/parameter-default sites call
        // `widen_mutable_binding_initializer_type` / freshness predicates
        // instead of consulting enum facts directly. The enum *facts*
        // themselves still come from `enum_analysis`, consumed inside
        // `types/utilities/core.rs` (the fresh enum-member-access probe and
        // the member-to-parent `DefId` resolution the widening helpers share).
        (
            "src/types/utilities/widening.rs",
            "enum_query::enum_member_parent_def_id(",
        ),
        (
            "src/types/utilities/core.rs",
            "enum_query::is_enum_member_for_widening(",
        ),
        (
            "src/types/property_access_type/helpers.rs",
            "enum_query::is_enum_member_for_widening(",
        ),
    ] {
        let source = fs::read_to_string(path).unwrap_or_else(|_| panic!("failed to read {path}"));
        assert!(
            source.contains(helper),
            "{path} should ask query_boundaries::enum_analysis via `{helper}`"
        );
    }
}

/// Full enum-member-union parent detection is a semantic enum fact. Indexed
/// access and diagnostic display callers should share the enum-analysis helper
/// instead of recounting enum exports locally.
#[test]
fn test_full_enum_member_union_parent_uses_enum_analysis_boundary() {
    let boundary_source = fs::read_to_string("src/query_boundaries/enum_analysis.rs")
        .expect("failed to read enum_analysis.rs");
    for helper in [
        "fn enum_member_like_parent_symbol(",
        "fn union_contains_all_members_of_enum(",
        "fn full_enum_member_union_parent_symbol(",
        "fn full_enum_member_union_parent_type(",
    ] {
        assert!(
            boundary_source.contains(helper),
            "query_boundaries::enum_analysis must own `{helper}`"
        );
    }

    let indexed_access_source =
        fs::read_to_string("src/types/type_node_advanced/enum_indexed_access.rs")
            .expect("failed to read enum_indexed_access.rs");
    assert!(
        indexed_access_source.contains("enum_query::full_enum_member_union_parent_type("),
        "enum indexed-access handling should ask enum_analysis for full enum-member union parents"
    );
    assert!(
        !indexed_access_source.contains("fn enum_parent_for_member_like_type("),
        "enum indexed-access handling should not locally rediscover member-like enum parents"
    );

    let display_source = fs::read_to_string("src/error_reporter/assignability_enum_display.rs")
        .expect("failed to read assignability_enum_display.rs");
    for helper in [
        "enum_query::full_enum_member_union_parent_symbol(",
        "enum_query::enum_member_like_parent_symbol(",
        "enum_query::union_contains_all_members_of_enum(",
    ] {
        assert!(
            display_source.contains(helper),
            "assignability enum display should call `{helper}`"
        );
    }
    for removed_helper in [
        "fn enum_member_symbol_for_type(",
        "fn union_contains_all_enum_members(",
    ] {
        assert!(
            !display_source.contains(removed_helper),
            "assignability enum display should not own `{removed_helper}`"
        );
    }
}

/// Numeric enum assignment diagnostics should ask the enum-analysis boundary
/// for target/literal facts, then use the assignability gateway only for the
/// final whole-enum member-value relation.
#[test]
fn test_numeric_enum_assignment_uses_enum_analysis_boundary() {
    let boundary_source = fs::read_to_string("src/query_boundaries/enum_analysis.rs")
        .expect("failed to read enum_analysis.rs");
    for helper in [
        "enum NumericEnumAssignmentTarget",
        "fn numeric_enum_assignment_target(",
        "fn numeric_literal_value(",
    ] {
        assert!(
            boundary_source.contains(helper),
            "query_boundaries::enum_analysis must own `{helper}`"
        );
    }

    let diagnostic_source = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability_diagnostics.rs");
    assert!(
        diagnostic_source.contains("enum_query::numeric_enum_assignment_target("),
        "assignability diagnostics should ask enum_analysis for numeric enum assignment targets"
    );
    assert!(
        diagnostic_source.contains("NumericEnumAssignmentTarget::Enum"),
        "assignability diagnostics should handle whole numeric enum targets from enum_analysis"
    );
    assert!(
        diagnostic_source.contains("NumericEnumAssignmentTarget::Member"),
        "assignability diagnostics should handle numeric enum member targets from enum_analysis"
    );

    let call_error_source = fs::read_to_string("src/error_reporter/call_errors/error_emission.rs")
        .expect("failed to read call error emission");
    assert!(
        call_error_source.contains("numeric_enum_assignment_override_from_source("),
        "TS2345 call emission should share the numeric enum assignment override"
    );
    for forbidden_probe in [
        "query_boundaries::diagnostics::enum_def_id",
        "query_boundaries::diagnostics::enum_member_type",
        "query_boundaries::diagnostics::literal_value",
        "ctx.is_numeric_enum",
        "ctx.is_enum_type",
    ] {
        assert!(
            !call_error_source.contains(forbidden_probe),
            "TS2345 call emission should not own enum semantic probe `{forbidden_probe}`"
        );
    }
}

/// Enum-member condition truthiness and TS2559 parent display are enum facts,
/// but their callers still own syntax, declaration fallback, and rendering.
#[test]
fn test_enum_truthiness_and_ts2559_use_enum_analysis_boundary() {
    let boundary_source = fs::read_to_string("src/query_boundaries/enum_analysis.rs")
        .expect("failed to read enum_analysis.rs");
    assert!(
        boundary_source.contains("fn enum_member_literal_condition_result("),
        "query_boundaries::enum_analysis must own enum-member literal truthiness"
    );

    let truthiness_source = fs::read_to_string("src/types/queries/callable_truthiness.rs")
        .expect("failed to read callable_truthiness.rs");
    assert!(
        truthiness_source.contains("enum_query::enum_member_literal_condition_result("),
        "callable truthiness should ask enum_analysis for enum-member literal truthiness"
    );
    for forbidden_probe in ["enum_member_type(", "classify_literal_type("] {
        assert!(
            !truthiness_source.contains(forbidden_probe),
            "callable truthiness should not own enum member literal probe `{forbidden_probe}`"
        );
    }

    let assignability_source = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability_diagnostics.rs");
    let ts2559_start = assignability_source
        .find("fn ts2559_source_display")
        .expect("missing ts2559_source_display");
    let ts2559_end = assignability_source[ts2559_start..]
        .find("pub(crate) fn error_no_common_properties")
        .map(|offset| ts2559_start + offset)
        .expect("missing error_no_common_properties");
    let ts2559_helper = &assignability_source[ts2559_start..ts2559_end];
    assert!(
        ts2559_helper.contains("enum_query::enum_member_like_parent_escaped_name("),
        "TS2559 enum-member parent display should ask enum_analysis"
    );
    for forbidden_probe in [
        "ctx.binder.get_symbol",
        "resolve_type_to_symbol_id",
        "symbol_flags::ENUM_MEMBER",
        ".parent",
    ] {
        assert!(
            !ts2559_helper.contains(forbidden_probe),
            "TS2559 source display should not locally inspect enum parent facts with `{forbidden_probe}`"
        );
    }
}

/// Declaration-order enum values need checker AST/binder access, so `enum_eval`
/// owns that fallback while callable truthiness owns only condition gating.
#[test]
fn test_enum_declared_value_fallback_uses_enum_eval_helper() {
    let enum_eval_source = fs::read_to_string("src/types/utilities/enum_eval.rs")
        .expect("failed to read enum_eval.rs");
    assert!(
        enum_eval_source.contains("enum EnumMemberConstValue"),
        "enum_eval should own the declared enum-member value representation"
    );
    assert!(
        enum_eval_source.contains("fn enum_member_declared_const_value("),
        "enum_eval should own declaration-order enum member value recovery"
    );
    assert!(
        enum_eval_source.contains("enum IsolatedEnumInitializerKind"),
        "enum_eval should own isolatedModules enum initializer classification"
    );
    assert!(
        enum_eval_source.contains("fn classify_isolated_enum_initializer(")
            && enum_eval_source.contains("fn is_numeric_constant_enum_expr("),
        "enum_eval should expose declaration-time enum initializer classification helpers"
    );
    assert!(
        enum_eval_source.contains("fn isolated_decl_enum_initializer_is_computable(")
            && enum_eval_source.contains("fn isolated_decl_enum_value_is_computable("),
        "enum_eval should own isolatedDeclarations enum initializer computability"
    );
    assert!(
        enum_eval_source.contains("fn enum_member_initializer_const_value("),
        "enum_eval should own enum member initializer value recovery"
    );
    assert!(
        enum_eval_source.contains("fn visit_enum_member_declared_const_values("),
        "enum_eval should expose one-pass declaration-order enum value recovery"
    );
    assert!(
        enum_eval_source.contains("fn enum_member_decl_for_name_in_data(")
            && enum_eval_source.contains("fn enum_symbol_member_decl_for_name(")
            && enum_eval_source.contains("fn enum_member_name("),
        "enum_eval should own enum declaration member-name lookup helpers"
    );
    assert!(
        enum_eval_source.contains("visit_enum_member_declared_const_values(sym_id,"),
        "enum compatibility maps should share the declaration-order value visitor"
    );
    assert!(
        enum_eval_source.contains("enum_member_decl_for_name_in_data(ctx, enum_data, name_text)"),
        "isolatedModules numeric enum classification should share enum member lookup"
    );

    let enum_utils_source = fs::read_to_string("src/types/utilities/enum_utils.rs")
        .expect("failed to read enum_utils.rs");
    assert!(
        enum_utils_source
            .contains("enum_symbol_member_decl_for_name(&self.ctx, sym_id, property_name)"),
        "enum member type lookup should ask enum_eval for declaration membership"
    );
    assert!(
        !enum_utils_source.contains("for &decl_idx in &symbol.declarations"),
        "enum_utils should not own a local enum declaration/member lookup scan"
    );

    let truthiness_source = fs::read_to_string("src/types/queries/callable_truthiness.rs")
        .expect("failed to read callable_truthiness.rs");
    assert!(
        truthiness_source.contains("enum_member_declared_const_value("),
        "callable truthiness should ask enum_eval for declaration fallback values"
    );
    for forbidden_helper in [
        "fn truthiness_for_enum_member_initializer(",
        "fn next_enum_auto_value(",
        "evaluate_constant_expression(initializer)",
    ] {
        assert!(
            !truthiness_source.contains(forbidden_helper),
            "callable truthiness should not own enum declaration fallback helper `{forbidden_helper}`"
        );
    }

    let declarations_source = fs::read_to_string("src/declarations/declarations.rs")
        .expect("failed to read declarations.rs");
    assert!(
        declarations_source.contains("classify_isolated_enum_initializer(")
            && declarations_source.contains("is_numeric_constant_enum_expr(self.ctx,"),
        "declaration checking should ask enum_eval for isolatedModules enum initializer facts"
    );
    assert!(
        declarations_source.contains("self.ctx")
            && declarations_source.contains("member_data.initializer"),
        "declaration checking should pass checker context and enum initializer nodes into enum_eval"
    );
    for forbidden_helper in [
        "fn classify_isolated_enum_initializer(",
        "fn is_numeric_constant_enum_expr(",
        "fn classify_symbol_backed_enum_initializer(",
        "fn classify_initializer_kind_in_arena(",
    ] {
        assert!(
            !declarations_source.contains(forbidden_helper),
            "declaration checking should not own enum initializer classifier `{forbidden_helper}`"
        );
    }

    let isolated_decl_source =
        fs::read_to_string("src/state/state_checking/isolated_declarations.rs")
            .expect("failed to read isolated_declarations.rs");
    assert!(
        isolated_decl_source.contains("isolated_decl_enum_initializer_is_computable(")
            && isolated_decl_source.contains("&self.ctx")
            && isolated_decl_source.contains("member.initializer"),
        "isolated declaration checking should ask enum_eval for enum initializer computability"
    );
    for forbidden_helper in [
        "fn is_isolated_decl_enum_value(",
        "fn is_same_enum_member_reference(",
        "fn is_same_enum_member_named_computable(",
        "fn is_same_enum_access(",
    ] {
        assert!(
            !isolated_decl_source.contains(forbidden_helper),
            "isolated declaration checking should not own enum computability helper `{forbidden_helper}`"
        );
    }

    let computed_symbol_source = fs::read_to_string("src/state/type_analysis/computed/mod.rs")
        .expect("failed to read computed symbol type source");
    assert!(
        computed_symbol_source.contains("self.enum_member_type_from_decl(member_idx)"),
        "enum type construction should ask enum_eval for member literal types"
    );
    for forbidden_fragment in [
        "let mut auto_value: Option<f64>",
        "self.evaluate_constant_expression(member.initializer)",
    ] {
        assert!(
            !computed_symbol_source.contains(forbidden_fragment),
            "enum type construction should not own enum value counter fragment `{forbidden_fragment}`"
        );
    }

    let type_environment_source = fs::read_to_string("src/state/type_environment/core.rs")
        .expect("failed to read type environment core source");
    assert!(
        type_environment_source.contains("checker.enum_member_type_from_decl(member_idx)"),
        "cross-file enum object construction should delegate enum member literal recovery"
    );
    assert!(
        type_environment_source.contains("CheckerCreationReason::TypeEnvironmentCore"),
        "cross-file enum object construction should attribute declaration-file delegation"
    );
    assert!(
        type_environment_source.contains("enter_delegate()"),
        "cross-file enum object construction should track delegation depth"
    );
    for forbidden_fragment in [
        "SyntaxKind::StringLiteral",
        "SyntaxKind::NumericLiteral",
        "parse_numeric_literal_value",
    ] {
        assert!(
            !type_environment_source.contains(forbidden_fragment),
            "enum object construction should not hand-parse enum member values with `{forbidden_fragment}`"
        );
    }
}

/// The `RelationFailure` enum must live in `relation_types.rs` and provide
/// structured variant coverage for the semantic families we're unifying.
#[test]
fn test_relation_failure_covers_semantic_families() {
    let source = fs::read_to_string("src/query_boundaries/relation_types.rs")
        .expect("failed to read query_boundaries/relation_types.rs");

    // Core semantic families that must be represented
    for variant in [
        "MissingProperty",
        "MissingProperties",
        "ExcessProperty",
        "IncompatiblePropertyValue",
        "NoApplicableSignature",
        "TupleArityMismatch",
        "ReturnTypeMismatch",
        "ParameterTypeMismatch",
        "ParameterCountMismatch",
        "PropertyModifierMismatch",
        "WeakUnionViolation",
        "TypeMismatch",
    ] {
        assert!(
            source.contains(variant),
            "RelationFailure must include the `{variant}` variant for semantic coverage"
        );
    }
}
