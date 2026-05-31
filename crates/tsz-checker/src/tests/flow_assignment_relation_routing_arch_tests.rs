use std::fs;

#[test]
fn flow_assignment_and_predicate_exclusion_use_relation_outcome_boundary() {
    let assignment_source = fs::read_to_string("src/flow/control_flow/assignment.rs")
        .expect("failed to read flow assignment source");
    let call_predicate_source =
        fs::read_to_string("src/flow/control_flow/call_condition_narrowing.rs")
            .expect("failed to read call predicate narrowing source");
    let type_guard_source = fs::read_to_string("src/flow/control_flow/type_guards.rs")
        .expect("failed to read type guard source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis query boundary");
    let compact_assignment: String = assignment_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_call_predicate: String = call_predicate_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_type_guard: String = type_guard_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("fnflow_assignability_outcome(")
            && compact_boundary.contains("RelationOutcome{related,"),
        "flow assignability truth should be exposed through an outcome-shaped query boundary"
    );
    assert!(
        compact_assignment.contains("assignment_relation_outcome(assigned_type,read_type,true)")
            && compact_assignment
                .contains("assignment_relation_outcome(assigned_type,write_type,true)")
            && compact_assignment
                .contains("assignment_relation_outcome(assigned_type,target_type,false)")
            && compact_assignment.contains("assignment_relation_outcome(rhs_type,lhs_type,false)")
            && compact_assignment
                .contains("assignment_relation_outcome(nullish_type,annotation_type,true)"),
        "flow assignment guards should consume outcome-shaped relation truth"
    );
    assert!(
        !compact_assignment.contains("self.is_assignable_to(")
            && !compact_assignment.contains("self.is_assignable_to_strict_null("),
        "flow assignment should not call raw flow assignability helpers directly"
    );
    assert!(
        compact_call_predicate.contains("flow_assignability_outcome(")
            && compact_call_predicate.contains(").related"),
        "call predicate exclusion should consume outcome-shaped relation truth"
    );
    assert!(
        !compact_call_predicate
            .contains(".filter(|member|self.is_assignable_to(*member,predicate_type))"),
        "call predicate exclusion should not filter union members with a raw relation call"
    );
    assert!(
        compact_type_guard.contains("flow_assignability_outcome(")
            && compact_type_guard.contains("arg_type,evaluated_pred,false")
            && compact_type_guard.contains(").related"),
        "generic predicate cache-skipping should consume outcome-shaped relation truth"
    );
    assert!(
        !compact_type_guard.contains("self.interner.is_assignable_to(arg_type,evaluated_pred)"),
        "generic predicate cache-skipping should not call raw interner assignability"
    );
}
