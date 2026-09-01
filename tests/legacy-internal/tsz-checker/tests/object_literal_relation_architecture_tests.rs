use std::fs;

/// Nested object-literal property diagnostics should choose the source and
/// property-name anchor, then let the canonical exact-anchor relation helper
/// own the TS2322 relation decision and rendering.
#[test]
fn test_nested_object_literal_property_diagnostics_use_exact_anchor_relation_helper() {
    let source = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability_diagnostics.rs");
    let nested_property_block = source
        .split("// Check nested object literal excess properties FIRST")
        .nth(1)
        .and_then(|tail| {
            tail.split("self.ctx.diagnostics.len() > diag_count_before")
                .next()
        })
        .expect("failed to locate nested object-literal property diagnostic block");

    assert!(
        nested_property_block
            .contains("check_assignable_or_report_at_exact_anchor_without_source_elaboration("),
        "nested object-literal property diagnostics must use the canonical exact-anchor \
         relation helper"
    );
    assert!(
        !nested_property_block.contains("diagnostic_relation_boolean_guard("),
        "nested object-literal property diagnostics must not pre-gate the canonical \
         relation helper with a raw boolean relation"
    );
    assert!(
        !nested_property_block.contains("error_type_not_assignable_at_with_anchor("),
        "nested object-literal property diagnostics must not bypass relation outcome \
         handling with the manual TS2322 reporter"
    );
}

/// Mapped object-literal property diagnostics need a display target distinct
/// from the relation target, but the relation decision still belongs in the
/// canonical exact-anchor helper.
#[test]
fn test_mapped_object_literal_property_diagnostics_use_display_relation_helper() {
    let source = fs::read_to_string("src/state/state_checking/mapped_object_literals.rs")
        .expect("failed to read mapped_object_literals.rs");

    assert!(
        source.contains(
            "check_assignable_or_report_at_exact_anchor_without_source_elaboration_with_display_types("
        ),
        "mapped object-literal property diagnostics must use the display-aware \
         exact-anchor relation helper"
    );
    assert!(
        !source.contains("error_type_not_assignable_at_with_anchor("),
        "mapped object-literal property diagnostics must not bypass relation outcome \
         handling with the manual TS2322 reporter"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard(source_prop_type, target_prop_type)")
            && !source.contains(
                "diagnostic_relation_boolean_guard(source_type, target_prop_type_for_check)"
            ),
        "mapped object-literal property diagnostics must not pre-gate the canonical \
         relation helper with raw boolean relations"
    );
}

/// Union index-signature property value mismatches need local index-signature
/// acceptance policy, but the final TS2322 diagnostic should still route
/// through the canonical exact-anchor helper.
#[test]
fn test_union_index_signature_property_mismatch_uses_relation_helper() {
    let source = fs::read_to_string(
        "src/state/state_checking/property/union_index_signature_diagnostics.rs",
    )
    .expect("failed to read union_index_signature_diagnostics.rs");

    assert!(
        source.contains("check_assignable_or_report_at_exact_anchor_without_source_elaboration("),
        "union index-signature property mismatches must use the canonical \
         exact-anchor relation helper for final TS2322 emission"
    );
    assert!(
        !source.contains("error_type_not_assignable_at_with_anchor("),
        "union index-signature property mismatches must not bypass relation outcome \
        handling with the manual TS2322 reporter"
    );
}

/// Object-literal property diagnostics must route declared-property and
/// contextual-property assignability through canonical relation diagnostic
/// helpers instead of raw boolean relations plus manual TS2322 reporting.
#[test]
fn test_object_literal_declared_property_uses_relation_diagnostic_helper() {
    let source = fs::read_to_string("src/types/computation/object_literal/computation.rs")
        .expect("failed to read object_literal/computation.rs");

    assert!(
        source.contains("check_assignable_or_report_at_exact_anchor_without_source_elaboration("),
        "object literal declared-property diagnostics must use the canonical \
         relation diagnostic helper"
    );
    assert!(
        source.contains("check_assignable_or_report_at_exact_anchor("),
        "object literal contextual-property diagnostics must use the canonical \
         relation diagnostic helper"
    );
    assert!(
        !source.contains("error_type_not_assignable_at_with_anchor("),
        "object literal declared-property diagnostics must not bypass relation \
         outcome handling with the manual TS2322 reporter"
    );
    let contextual_recheck = source
        .split("let recheck_key_remapped_property")
        .nth(1)
        .and_then(|tail| tail.split("// Freshness model").next())
        .expect("failed to locate object-literal contextual-property recheck block");
    assert!(
        !contextual_recheck.contains("!self.is_assignable_to("),
        "object literal contextual-property diagnostics must not pre-gate the \
         canonical relation diagnostic helper with raw is_assignable_to"
    );
    let jsdoc_declared_property = source
        .split("let value_type = if let Some(declared_type) = jsdoc_declared_type")
        .nth(1)
        .and_then(|tail| tail.split("let value_has_non_widening_source").next())
        .expect("failed to locate object-literal JSDoc declared-property block");
    assert!(
        jsdoc_declared_property
            .contains("object_literal_jsdoc_declared_property_relation_outcome("),
        "object literal JSDoc declared-property diagnostics must pre-gate \
         through RelationOutcome"
    );
    assert!(
        !jsdoc_declared_property.contains("assign_relation_outcome(value_type, declared_type)")
            && !jsdoc_declared_property.contains("assign_relation_outcome("),
        "object literal JSDoc declared-property diagnostics must route through \
         a named RelationRequest"
    );
    assert!(
        !jsdoc_declared_property.contains("!self.is_assignable_to(value_type, declared_type)"),
        "object literal JSDoc declared-property diagnostics must not pre-gate \
         the canonical relation diagnostic helper with raw is_assignable_to"
    );
}

/// Mapped contextual object-literal property lookup should keep checker-owned
/// template instantiation local, but route the key-space relation probes through
/// a named relation request.
#[test]
fn test_mapped_contextual_property_type_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/object_literal_context.rs")
        .expect("failed to read object_literal_context.rs");
    let helper = source
        .split("fn mapped_contextual_property_type")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn contextual_object_literal_property_type")
                .next()
        })
        .expect("failed to locate mapped contextual property helper");

    assert_eq!(
        helper
            .matches("object_literal_mapped_contextual_key_relation_outcome(")
            .count(),
        2,
        "mapped contextual property key checks must route through the mapped contextual key request"
    );
    assert!(
        helper.matches(".related").count() >= 2,
        "mapped contextual property key checks must use relation outcome decisions"
    );
    assert!(
        !helper.contains("assign_relation_outcome(")
            && !helper.contains("is_assignable_to(key_type,"),
        "mapped contextual property key checks must not regress to raw assignability probes"
    );
}

/// Contextual symbol-index value diagnostics choose the computed-name anchor
/// locally, but the source-to-symbol-index relation decision belongs on the
/// shared relation outcome path.
#[test]
fn test_contextual_symbol_index_value_mismatch_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/object_literal/symbol_key_routing.rs")
        .expect("failed to read symbol_key_routing.rs");
    let helper = source
        .split("fn report_contextual_symbol_index_value_mismatch")
        .nth(1)
        .and_then(|tail| tail.split("fn contextual_symbol_index_value_type").next())
        .expect("failed to locate contextual symbol index diagnostic helper");

    assert!(
        helper.contains(
            "contextual_symbol_index_value_relation_outcome(source_value_type, target_value_type)"
        ) && helper.contains(".related"),
        "contextual symbol-index diagnostics must route value compatibility through the dedicated relation outcome"
    );
    assert!(
        !helper.contains("assign_relation_outcome("),
        "contextual symbol-index diagnostics must not use the generic assign relation outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard(source_value_type, target_value_type)"),
        "contextual symbol-index diagnostics must not use a raw diagnostic boolean relation guard"
    );
}

/// Computed-key index-signature routing chooses the number/symbol/string bucket
/// locally, but key-space relation decisions belong on the shared relation
/// outcome path.
#[test]
fn test_computed_key_index_signature_routing_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/object_literal/symbol_key_routing.rs")
        .expect("failed to read symbol_key_routing.rs");
    let helper = source
        .split("fn route_computed_member_value_to_index_signature")
        .nth(1)
        .and_then(|tail| tail.split("fn computed_name_is_wide_symbol").next())
        .expect("failed to locate computed-key index-signature routing helper");

    assert!(
        helper.contains(
            "object_literal_computed_key_relation_outcome(prop_name_type, TypeId::NUMBER)"
        ) && helper.contains(
            "object_literal_computed_key_relation_outcome(prop_name_type, TypeId::SYMBOL)"
        ) && helper.contains(".related"),
        "computed-key index-signature routing must route key-space checks through the dedicated relation outcome"
    );
    assert!(
        !helper.contains("assign_relation_outcome("),
        "computed-key index-signature routing must not use the generic assign relation outcome"
    );
    assert!(
        !helper.contains("is_assignable_to(prop_name_type, TypeId::NUMBER)")
            && !helper.contains("is_assignable_to(prop_name_type, TypeId::SYMBOL)"),
        "computed-key index-signature routing must not regress to raw boolean assignability"
    );
}

#[test]
fn test_object_literal_symbol_key_helpers_use_dedicated_requests() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn object_literal_computed_key_relation_outcome(")
            && source.contains("RelationRequest::object_literal_computed_key("),
        "computed-key routing should have a dedicated RelationRequest helper"
    );
    assert!(
        source.contains("fn object_literal_mapped_contextual_key_relation_outcome(")
            && source.contains("RelationRequest::object_literal_mapped_contextual_key("),
        "mapped contextual property key lookup should have a dedicated RelationRequest helper"
    );
    assert!(
        source.contains("fn contextual_symbol_index_value_relation_outcome(")
            && source.contains("RelationRequest::contextual_symbol_index_value("),
        "contextual symbol-index diagnostics should have a dedicated RelationRequest helper"
    );
}

/// Excess-property discriminant narrowing owns candidate filtering locally, but
/// discriminator subtype probes should still use outcome-shaped relation truth.
#[test]
fn test_excess_property_discriminant_filters_use_subtype_outcome_boundary() {
    let source = fs::read_to_string("src/state/state_checking/property/excess_property_tail.rs")
        .expect("failed to read excess_property_tail.rs");
    let compact = source.split_whitespace().collect::<String>();

    assert!(
        compact.contains("diagnostic_subtype_outcome(prop_type,*target_ty).related")
            && compact.contains("diagnostic_subtype_outcome(prop_type,prop.type_id).related"),
        "excess-property discriminant filters must route subtype probes through outcomes"
    );
    assert!(
        !compact.contains("is_subtype_of(prop_type,*target_ty)")
            && !compact.contains("is_subtype_of(prop_type,prop.type_id)"),
        "excess-property discriminant filters must not consume raw subtype booleans directly"
    );
}

/// Contextual object-literal union selection keeps discriminator matching local,
/// but unit-value subtype checks should still use outcome-shaped relation truth.
#[test]
fn test_contextual_object_literal_discriminants_use_subtype_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/object_literal_context.rs")
        .expect("failed to read object_literal_context.rs");
    let helper = source
        .split("fn narrow_contextual_union_via_object_literal_discriminants")
        .nth(1)
        .and_then(|tail| tail.split("fn shorthand_const_literal_type").next())
        .expect("failed to locate contextual object-literal discriminant helper");
    let compact = helper.split_whitespace().collect::<String>();

    assert!(
        compact.contains("diagnostic_subtype_outcome(*lit_type,target_type).related"),
        "contextual object-literal discriminant checks must route subtype probes through outcomes"
    );
    assert!(
        !compact.contains("is_subtype_of(*lit_type,target_type)"),
        "contextual object-literal discriminant checks must not consume raw subtype booleans directly"
    );
}
