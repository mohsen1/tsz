use std::fs;

#[test]
fn function_type_predicate_validation_uses_relation_outcome_boundary() {
    let relation_source = fs::read_to_string("src/assignability/assignability_relation.rs")
        .expect("failed to read assignability_relation.rs");
    let function_checks_source =
        fs::read_to_string("src/state/state_checking_members/function_declaration_checks.rs")
            .expect("failed to read function_declaration_checks.rs");
    let type_node_source =
        fs::read_to_string("src/types/type_node.rs").expect("failed to read type_node.rs");
    let boundary_source = fs::read_to_string("src/query_boundaries/type_predicates.rs")
        .expect("failed to read type_predicates.rs");
    let compact_relation: String = relation_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_type_node: String = type_node_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        function_checks_source.contains("type_predicate_type_assignable_to_parameter"),
        "function declaration predicate validation should use the checker relation helper"
    );
    assert!(
        compact_relation.contains(
            "|source,target|{self.type_predicate_parameter_relation_outcome(source,target).related}"
        ),
        "checker-state type-predicate validation should provide type-predicate parameter RelationOutcome truth"
    );
    assert!(
        !compact_relation.contains("|source,target|self.assign_relation_outcome(source,target)"),
        "checker-state type-predicate validation should not use generic assign requests"
    );
    assert!(
        !compact_relation.contains("|source,target|self.is_assignable_to(source,target)"),
        "checker-state type-predicate validation should not provide raw assignability truth"
    );
    assert!(
        compact_type_node.contains("type_predicate_type_assignability_outcome(")
            && compact_type_node.contains("types,&*self.ctx,resolved_predicate,resolved_param")
            && compact_type_node.contains(").related"),
        "type-node predicate validation should consume the outcome-shaped predicate boundary and thread the checker resolver"
    );
    assert!(
        !compact_type_node.contains("|source,target|types.is_assignable_to(source,target)"),
        "type-node predicate validation should not pass raw TypeDatabase assignability from checker code"
    );
    assert!(
        compact_boundary.contains("fntype_predicate_relation_outcome<R:TypeResolver>(")
            && compact_boundary.contains(
                "|source,target|type_predicate_relation_outcome(db,resolver,source,target).related"
            ),
        "type-node predicate boundary should route recursive relation probes through a RelationOutcome with the threaded resolver"
    );
    assert!(
        !compact_boundary.contains("db.is_assignable_to(source,target)"),
        "type-node predicate boundary should not call raw TypeDatabase assignability directly"
    );
}
