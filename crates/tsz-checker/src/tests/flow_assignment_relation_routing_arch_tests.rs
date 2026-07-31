use std::fs;

#[test]
fn flow_assignment_and_predicate_exclusion_use_relation_outcome_boundary() {
    // `core.rs` was split into a `core/` submodule (e.g. `core/flow_query.rs`,
    // `core/flow_traversal.rs`), so the flow-analyzer relation decisions this
    // test pins now live across that module tree. Read `core.rs` plus every
    // sibling source under `core/` so the routing contract is checked wherever
    // the call sites land, not just in the historic single file.
    let mut core_source = fs::read_to_string("src/flow/control_flow/core.rs")
        .expect("failed to read flow analyzer core source");
    // Order is irrelevant — every assertion below is a `contains` check — so
    // concatenate the submodule sources directly without sorting.
    if let Ok(entries) = fs::read_dir("src/flow/control_flow/core") {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "rs")
                && let Ok(contents) = fs::read_to_string(&path)
            {
                core_source.push('\n');
                core_source.push_str(&contents);
            }
        }
    }
    // `whole_assignment_rhs_is_compatible`/`pack_relation_flags` call sites moved
    // into the sibling `assignment_compatibility.rs` module (whole-RHS fallback
    // validation split out of `assignment.rs`), so pin both sources together.
    let mut assignment_source = fs::read_to_string("src/flow/control_flow/assignment.rs")
        .expect("failed to read flow assignment source");
    assignment_source.push('\n');
    assignment_source.push_str(
        &fs::read_to_string("src/flow/control_flow/assignment_compatibility.rs")
            .expect("failed to read flow assignment compatibility source"),
    );
    let call_predicate_source =
        fs::read_to_string("src/flow/control_flow/call_condition_narrowing.rs")
            .expect("failed to read call predicate narrowing source");
    let type_guard_source = fs::read_to_string("src/flow/control_flow/type_guards.rs")
        .expect("failed to read type guard source");
    let reachability_source = fs::read_to_string("src/flow/reachability_checker.rs")
        .expect("failed to read reachability checker source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis query boundary");
    let compact_core: String = core_source.chars().filter(|c| !c.is_whitespace()).collect();
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
    let compact_reachability: String = reachability_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let whole_rhs_start = boundary_source
        .find("pub(crate) fn whole_assignment_rhs_is_compatible(")
        .expect("whole-RHS relation boundary must exist");
    let whole_rhs_tail = &boundary_source[whole_rhs_start..];
    let whole_rhs_end = whole_rhs_tail
        .find("\nfn substitute_flow_this_type(")
        .expect("whole-RHS relation boundary must end before the next helper");
    let compact_whole_rhs: String = whole_rhs_tail[..whole_rhs_end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("fnflow_assignability_outcome(")
            && compact_boundary.contains("RelationOutcome{related,"),
        "flow assignability truth should be exposed through an outcome-shaped query boundary"
    );
    assert!(
        compact_boundary.contains("fnflow_relation_outcome(")
            && compact_boundary.contains("fnflow_relation_related(")
            && compact_boundary.contains("query_relation_with_resolver(")
            && compact_boundary
                .contains("flow_relation_outcome(db,env,source,member,true).related")
            && compact_boundary.contains(
                "flow_relation_outcome(db,None,assigned_resolved,resolved_initial,false).related"
            ),
        "flow assignment reduction should consume outcome-shaped resolver-aware relation truth"
    );
    assert!(
        compact_core.contains("fnflow_assignability_related(")
            && compact_core.contains("query::flow_assignability_outcome(")
            && compact_core.contains("source,target,false,).related"),
        "FlowAnalyzer boolean relation helper should stay backed by RelationOutcome.related"
    );
    assert!(
        compact_core.contains("flow_assignability_related(left,right)")
            && compact_core.contains("flow_assignability_related(right,left)"),
        "flow analyzer relation decisions should route through the outcome-backed helper"
    );
    assert!(
        !compact_core.contains("query::is_assignable_with_env(")
            && !compact_core.contains("query::is_assignable_strict_null("),
        "FlowAnalyzer assignability helpers should not call raw boolean relation boundaries"
    );
    assert!(
        !compact_core.contains("fnis_assignable_to(")
            && !compact_core.contains("self.is_assignable_to(")
            && !compact_core.contains("fnis_assignable_to_strict_null(")
            && !compact_core.contains("self.is_assignable_to_strict_null("),
        "flow analyzer should not route relation decisions through raw boolean shims"
    );
    assert!(
        compact_assignment.contains("assignment_relation_outcome(assigned_type,read_type,true)")
            && compact_assignment
                .contains("assignment_relation_outcome(assigned_type,write_type,true)")
            && compact_assignment
                .contains("assignment_relation_outcome(assigned_type,target_type,false)")
            && compact_assignment.contains("whole_assignment_rhs_is_compatible(")
            && compact_assignment.contains("ctx.pack_relation_flags()")
            && compact_assignment
                .contains("assignment_relation_outcome(nullish_type,annotation_type,true)"),
        "flow assignment guards should consume outcome-shaped relation truth"
    );
    assert!(
        compact_whole_rhs.contains("fnwhole_assignment_rhs_is_compatible(")
            && compact_whole_rhs.contains("relation_policy::from_checker_flags_u16(flags)")
            && compact_whole_rhs.contains("query_relation_with_resolver(")
            && compact_whole_rhs.contains("query_relation(")
            && compact_whole_rhs.contains("members.iter().copied().all(related)"),
        "whole-RHS assignment validity must apply the complete checker relation policy through resolver and no-resolver paths"
    );
    assert!(
        !compact_assignment.contains("self.is_assignable_to(")
            && !compact_assignment.contains("self.is_assignable_to_strict_null("),
        "flow assignment should not call raw flow assignability helpers directly"
    );
    assert!(
        compact_call_predicate.contains("flow_query::narrow_call_predicate_guard(")
            && compact_boundary.contains("fnnarrow_call_predicate_guard(")
            && compact_boundary.contains("flow_assignability_outcome(")
            && compact_boundary.contains(").related"),
        "call predicate exclusion should route through a flow_analysis helper that consumes outcome-shaped relation truth"
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
    assert!(
        compact_reachability.contains("flow_assignability_outcome(")
            && compact_reachability.contains("normalized_switch,cases_union")
            && compact_reachability.contains(").related"),
        "switch reachability fallback should consume outcome-shaped relation truth"
    );
    assert!(
        !compact_reachability.contains("is_assignable_with_env("),
        "switch reachability fallback should not call the raw env relation helper"
    );
}
