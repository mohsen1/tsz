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
    let compact_relation: String = relation_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_type_node: String = type_node_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        function_checks_source.contains("type_predicate_type_assignable_to_parameter"),
        "function declaration predicate validation should use the checker relation helper"
    );
    assert!(
        compact_relation
            .contains("|source,target|self.assign_relation_outcome(source,target).related"),
        "checker-state type-predicate validation should provide outcome-shaped relation truth"
    );
    assert!(
        !compact_relation.contains("|source,target|self.is_assignable_to(source,target)"),
        "checker-state type-predicate validation should not provide raw assignability truth"
    );
    assert!(
        compact_type_node.contains("type_predicate_type_assignability_outcome(")
            && compact_type_node.contains("types,resolved_predicate,resolved_param")
            && compact_type_node.contains(").related"),
        "type-node predicate validation should consume the outcome-shaped predicate boundary"
    );
    assert!(
        !compact_type_node.contains("|source,target|types.is_assignable_to(source,target)"),
        "type-node predicate validation should not pass raw TypeDatabase assignability from checker code"
    );
}
