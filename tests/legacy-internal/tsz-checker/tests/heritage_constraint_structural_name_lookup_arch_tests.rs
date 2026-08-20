/// Heritage type-name resolution for generic constraints must use structural
/// identity, not rendered display strings.
///
/// `symbol_id_for_heritage_type_name` is called as a last-resort fallback when
/// checking whether a type satisfies a heritage constraint. The previous
/// implementation called `format_type_diagnostic`, stripped a possible
/// `"globalThis."` prefix, and validated identifier characters — all artefacts
/// of using the printer as an identity oracle.
///
/// The correct approach is `named_type_display_name`, which returns the type's
/// declared identifier name from structural symbol/def/shape queries (or `None`
/// for unnamed types). This is deterministic, printer-independent, and avoids
/// the character-set guard that was needed only to filter out rendered non-names.
#[test]
fn heritage_type_name_resolution_uses_structural_lookup() {
    let src = include_str!("../checkers/generic_checker/recursive_heritage_constraint.rs");

    assert!(
        !src.contains("self.format_type_diagnostic(type_id)"),
        "`symbol_id_for_heritage_type_name` must not use `format_type_diagnostic` \
         to derive the heritage type name; use `named_type_display_name` instead"
    );

    assert!(
        src.contains("self.named_type_display_name(type_id)"),
        "`symbol_id_for_heritage_type_name` must resolve the type name structurally \
         via `named_type_display_name`"
    );

    assert!(
        !src.contains("strip_prefix(\"globalThis.\")"),
        "`symbol_id_for_heritage_type_name` must not strip a `globalThis.` prefix; \
         `named_type_display_name` produces bare identifier names without renderer artefacts"
    );
}

#[test]
fn recursive_heritage_property_conflicts_use_relation_outcome_boundary() {
    let src = include_str!("../query_boundaries/assignability.rs");
    let helper = src
        .split("pub(crate) fn recursive_heritage_property_types_conflict")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) fn mutable_array_element_for_redeclaration")
                .next()
        })
        .expect("failed to locate recursive heritage property conflict helper");

    assert!(
        helper
            .contains("recursive_heritage_property_relation_outcome(member_type, constraint_type)")
            && helper.contains(
                "recursive_heritage_property_relation_outcome(constraint_type, member_type)",
            )
            && helper.contains(".related"),
        "recursive heritage property conflicts must route relation truth through the recursive heritage property RelationRequest"
    );
    assert!(
        !helper.contains("assign_relation_outcome("),
        "recursive heritage property conflicts must not use generic assignment request routing"
    );
    assert!(
        !helper.contains("checker.is_assignable_to("),
        "recursive heritage property conflicts must not regress to raw checker assignability"
    );
}

#[test]
fn heritage_symbol_walks_use_named_visit_state() {
    let recursive_heritage =
        include_str!("../checkers/generic_checker/recursive_heritage_constraint.rs");
    let type_utilities = include_str!("../types/utilities/core.rs");
    let walk_state = include_str!("../types/utilities/heritage_walk_state.rs");

    assert!(
        recursive_heritage.contains("HeritageSymbolWalkState::new()")
            && recursive_heritage.contains("walk_state.mark_seen(interface_sym_id)")
            && type_utilities.contains("HeritageSymbolWalkState::new()")
            && type_utilities.contains("walk_state.enter_path(sym_id)")
            && type_utilities.contains("walk_state.leave_path(sym_id)"),
        "checker heritage symbol walks should route visit ownership through `HeritageSymbolWalkState`"
    );
    assert!(
        !recursive_heritage.contains("&mut Vec<tsz_binder::SymbolId>")
            && !recursive_heritage.contains("seen.contains(&interface_sym_id)")
            && !type_utilities.contains("let mut visited = Vec::new()")
            && !type_utilities.contains("visited: &mut Vec<SymbolId>")
            && !type_utilities.contains("visited.contains(&sym_id)"),
        "heritage walks should not thread raw symbol vectors through recursion"
    );
    assert!(
        walk_state.contains("pub(crate) struct HeritageSymbolWalkState")
            && walk_state.contains("pub(crate) fn mark_seen")
            && walk_state.contains("pub(crate) fn enter_path")
            && walk_state.contains("pub(crate) fn leave_path"),
        "`HeritageSymbolWalkState` must own both seen-set and path-stack visit semantics"
    );
}
