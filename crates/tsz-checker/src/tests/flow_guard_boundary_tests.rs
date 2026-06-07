//! Focused coverage for flow guard narrowing routed through query boundaries.

use std::fs;

use crate::test_utils::check_source_strict_codes as check_strict;

#[test]
fn assertion_type_predicate_narrows_after_call() {
    let codes = check_strict(
        r#"
declare function assertString(value: unknown): asserts value is string;

function use(value: unknown) {
    assertString(value);
    const text: string = value;
    text.toUpperCase();
}
"#,
    );

    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "expected assertion predicate narrowing to make value string, got codes: {codes:?}"
    );
}

#[test]
fn condition_type_predicate_narrows_false_branch() {
    let codes = check_strict(
        r#"
declare function isString(value: unknown): value is string;

function use(value: string | number) {
    if (isString(value)) {
        const text: string = value;
        text.toUpperCase();
    } else {
        const count: number = value;
        count.toFixed();
    }
}
"#,
    );

    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "expected type predicate narrowing in both branches, got codes: {codes:?}"
    );
}

#[test]
fn type_predicate_true_branch_removes_null_before_property_access() {
    let codes = check_strict(
        r#"
interface Node {
    nodeType: number;
}

interface Element extends Node {
    tagName: string;
}

function isElement(node: Node | null): node is Element {
    return node !== null && node.nodeType === 1;
}

function use(node: Node | null) {
    if (isElement(node)) {
        node.tagName.toLowerCase();
    }
}
"#,
    );

    assert!(
        !codes.contains(&18047) && !codes.contains(&2339),
        "expected type predicate to remove null and expose Element properties, got codes: {codes:?}"
    );
}

#[test]
fn instanceof_condition_narrows_both_branches() {
    let codes = check_strict(
        r#"
class Box {
    value = 1;
}

function use(value: Box | string) {
    if (value instanceof Box) {
        value.value;
    } else {
        value.toUpperCase();
    }
}
"#,
    );

    assert!(
        !codes.contains(&2339),
        "expected instanceof narrowing in both branches, got codes: {codes:?}"
    );
}

#[test]
fn in_condition_narrows_true_and_false_branches() {
    let codes = check_strict(
        r#"
function read(value: { present: string } | { absent: number }) {
    if ("present" in value) {
        const present: string = value.present;
        present.toUpperCase();
    } else {
        const absent: number = value.absent;
        absent.toFixed();
    }
}
"#,
    );

    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "expected `in` narrowing in both branches, got codes: {codes:?}"
    );
}

#[test]
fn in_condition_narrowing_routes_through_flow_query_boundary() {
    let narrowing_source = fs::read_to_string("src/flow/control_flow/narrowing.rs")
        .expect("failed to read flow narrowing source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_narrowing: String = narrowing_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("fnnarrow_in_property(")
            && compact_boundary.contains("&TypeGuard::InProperty(property_name)"),
        "`in` property flow narrowing should expose a dedicated query-boundary helper"
    );
    assert!(
        compact_narrowing.contains("flow_query::narrow_in_property("),
        "checker `in` narrowing should route semantic guard application through the flow query boundary"
    );
    assert!(
        !compact_narrowing.contains("narrowing.narrow_type(type_id,&TypeGuard::InProperty("),
        "checker `in` narrowing should not construct and apply `InProperty` guards locally"
    );
}

#[test]
fn definite_assignment_undefined_skip_uses_flow_query_boundary() {
    let usage_source = fs::read_to_string("src/flow/flow_analysis/usage.rs")
        .expect("failed to read flow usage source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_usage: String = usage_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("type_contains_undefined,"),
        "flow analysis boundary should expose the solver-owned undefined predicate"
    );
    assert!(
        compact_usage.contains("query_boundaries::flow_analysis::type_contains_undefined("),
        "TS2454 flow usage should ask the flow query boundary for undefined membership"
    );
    assert!(
        !compact_usage.contains("tsz_solver::narrowing::type_contains_undefined"),
        "flow usage should not import the solver narrowing predicate directly"
    );
}

#[test]
fn direct_source_file_optional_param_undefined_check_uses_query_boundary() {
    let source = fs::read_to_string("src/state/type_analysis/cross_file_direct_functions.rs")
        .expect("failed to read direct source-file function lowering source");
    let compact_source: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact_source.contains("query_boundaries::type_predicates::type_contains_undefined("),
        "direct source-file optional parameter lowering should ask a query boundary for undefined membership"
    );
    assert!(
        !compact_source.contains("tsz_solver::narrowing::type_contains_undefined"),
        "direct source-file lowering should not call the solver narrowing predicate directly"
    );
}

#[test]
fn condition_false_branch_falsy_narrowing_uses_flow_query_boundary() {
    let condition_source = fs::read_to_string("src/flow/control_flow/condition_narrowing.rs")
        .expect("failed to read condition narrowing source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_condition: String = condition_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("fnnarrow_to_falsy("),
        "flow analysis boundary should expose solver-owned falsy narrowing"
    );
    assert!(
        compact_condition.contains("flow_query::narrow_to_falsy("),
        "condition false-branch truthiness narrowing should route through the flow query boundary"
    );
    assert!(
        !compact_condition.contains(".narrow_to_falsy(type_id)"),
        "condition narrowing should not call solver falsy narrowing directly"
    );
}

#[test]
fn condition_typeof_narrowing_uses_flow_query_boundary() {
    let condition_source = fs::read_to_string("src/flow/control_flow/condition_narrowing.rs")
        .expect("failed to read condition narrowing source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_condition: String = condition_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("fnnarrow_by_typeof_result("),
        "flow analysis boundary should expose solver-owned typeof result narrowing"
    );
    assert!(
        compact_condition.contains("flow_query::narrow_by_typeof_result("),
        "condition typeof narrowing should route through the flow query boundary"
    );
    assert!(
        !compact_condition.contains(".narrow_by_typeof(")
            && !compact_condition.contains(".narrow_by_typeof_negation("),
        "condition narrowing should not call solver typeof narrowing directly"
    );
}

#[test]
fn condition_guard_application_uses_flow_query_boundary() {
    let condition_source = fs::read_to_string("src/flow/control_flow/condition_narrowing.rs")
        .expect("failed to read condition narrowing source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_condition: String = condition_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("fnnarrow_with_guard(")
            && compact_boundary.contains("narrowing.narrow_type(type_id,guard,"),
        "flow analysis boundary should own reusable solver guard application"
    );
    assert!(
        compact_condition.contains("self.narrow_with_guard_via_flow_boundary("),
        "condition guard application should route through the flow query boundary"
    );
    assert!(
        !compact_condition.contains(".narrow_type("),
        "condition narrowing should not apply solver guards directly"
    );
}

#[test]
fn condition_truthiness_payload_uses_flow_query_boundary() {
    let condition_source = fs::read_to_string("src/flow/control_flow/condition_narrowing.rs")
        .expect("failed to read condition narrowing source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_condition: String = condition_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let condition_truthiness_fn = compact_condition
        .split("fnnarrow_logical_assignment_condition(")
        .next()
        .and_then(|before_assignment| before_assignment.split("_=>{letcondition_ref=").nth(1))
        .expect("failed to locate condition truthiness narrowing body");
    let logical_assignment_body = compact_condition
        .split("ifcrate::query_boundaries::operator_wrappers::is_logical_compound_assignment_operator(")
        .nth(1)
        .expect("failed to locate logical assignment truthiness body");

    assert!(
        compact_boundary.contains("fnnarrow_to_truthy_in_context(")
            && compact_boundary.contains("&TypeGuard::Truthy"),
        "flow analysis boundary should own truthiness guard payload construction"
    );
    assert!(
        condition_truthiness_fn.contains("flow_query::narrow_to_truthy_in_context(")
            && logical_assignment_body.contains("flow_query::narrow_to_truthy_in_context("),
        "condition truthiness callers should route truthiness payloads through the flow query boundary"
    );
    assert!(
        !condition_truthiness_fn.contains("&TypeGuard::Truthy")
            && !logical_assignment_body.contains("&TypeGuard::Truthy"),
        "condition truthiness callers should not construct solver truthiness payloads locally"
    );
}

#[test]
fn condition_property_truthiness_uses_flow_query_boundary() {
    let condition_source = fs::read_to_string("src/flow/control_flow/condition_narrowing.rs")
        .expect("failed to read condition narrowing source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_condition: String = condition_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("fnnarrow_by_property_truthiness_in_context(")
            && compact_boundary.contains("narrowing.narrow_by_property_truthiness("),
        "flow analysis boundary should own property-truthiness narrowing"
    );
    assert!(
        compact_condition.contains("flow_query::narrow_by_property_truthiness_in_context("),
        "condition property truthiness should route through the flow query boundary"
    );
    assert!(
        !compact_condition.contains(".narrow_by_property_truthiness("),
        "condition narrowing should not apply property-truthiness narrowing directly"
    );
}

#[test]
fn condition_batched_exclusions_use_flow_query_boundary() {
    let condition_source = fs::read_to_string("src/flow/control_flow/condition_narrowing.rs")
        .expect("failed to read condition narrowing source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_condition: String = condition_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("fnnarrow_excluding_types_in_context(")
            && compact_boundary.contains("fnnarrow_by_excluding_discriminant_values_in_context("),
        "flow analysis boundary should own batched exclusion narrowing helpers"
    );
    assert!(
        compact_condition.contains("flow_query::narrow_excluding_types_in_context(")
            && compact_condition
                .contains("flow_query::narrow_by_excluding_discriminant_values_in_context("),
        "condition batched exclusion narrowing should route through the flow query boundary"
    );
    assert!(
        !compact_condition.contains("narrowing.narrow_excluding_types(")
            && !compact_condition.contains("narrowing.narrow_by_excluding_discriminant_values("),
        "condition narrowing should not apply batched exclusion narrowing directly"
    );
}

#[test]
fn condition_equality_narrowing_uses_flow_query_boundary() {
    let condition_source = fs::read_to_string("src/flow/control_flow/condition_narrowing.rs")
        .expect("failed to read condition narrowing source");
    let core_source = fs::read_to_string("src/flow/control_flow/core.rs")
        .expect("failed to read flow core source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_condition: String = condition_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_core: String = core_source.chars().filter(|c| !c.is_whitespace()).collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        compact_boundary.contains("fnnarrow_excluding_type_in_context(")
            && compact_boundary.contains("fnnarrow_by_discriminant_for_type_in_context(")
            && compact_boundary.contains("fnnarrow_to_type_in_context(")
            && compact_boundary.contains("fnliteral_assignable_to_in_context("),
        "flow analysis boundary should own equality/discriminant narrowing helpers"
    );
    assert!(
        compact_condition.contains("flow_query::narrow_excluding_type_in_context(")
            && compact_condition
                .contains("flow_query::narrow_by_discriminant_for_type_in_context(")
            && compact_condition.contains("flow_query::narrow_to_type_in_context(")
            && compact_condition.contains("flow_query::literal_assignable_to_in_context("),
        "condition equality narrowing should route semantic narrowing through the flow query boundary"
    );
    assert!(
        compact_core.contains("query::narrow_by_discriminant_in_context(")
            && compact_core.contains("query::narrow_with_guard_in_context("),
        "assertion flow narrowing should route solver predicate application through the flow query boundary"
    );
    assert!(
        !compact_condition.contains("narrowing.narrow_excluding_type(")
            && !compact_condition.contains("narrowing.narrow_by_discriminant_for_type(")
            && !compact_condition.contains("narrowing.narrow_to_type(")
            && !compact_condition.contains("narrowing.literal_assignable_to(")
            && !compact_core.contains("narrowing.narrow_by_discriminant(")
            && !compact_core.contains("narrowing.narrow_type("),
        "flow orchestration should not call solver equality/discriminant narrowing directly"
    );
}

#[test]
fn predicate_payload_application_uses_flow_query_boundary() {
    let narrowing_source = fs::read_to_string("src/flow/control_flow/narrowing.rs")
        .expect("failed to read flow narrowing source");
    let call_source = fs::read_to_string("src/flow/control_flow/call_condition_narrowing.rs")
        .expect("failed to read call condition narrowing source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_narrowing: String = narrowing_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_call: String = call_source.chars().filter(|c| !c.is_whitespace()).collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let predicate_fn = compact_narrowing
        .split("fnnarrow_by_instanceof(")
        .next()
        .and_then(|before_instanceof| {
            before_instanceof
                .split("fnapply_type_predicate_narrowing(")
                .nth(1)
        })
        .expect("failed to locate apply_type_predicate_narrowing body");

    assert!(
        compact_boundary.contains("fnnarrow_type_predicate(")
            && compact_boundary.contains("&TypeGuard::Predicate{")
            && compact_boundary.contains("fnnarrow_asserts_truthy(")
            && compact_boundary.contains("&TypeGuard::Truthy")
            && compact_boundary.contains("fnnarrow_property_type_by_predicate("),
        "flow analysis boundary should own predicate payload construction"
    );
    assert!(
        predicate_fn.contains("flow_query::narrow_type_predicate(")
            && predicate_fn.contains("flow_query::narrow_asserts_truthy(")
            && compact_call.contains("flow_query::narrow_property_type_by_predicate("),
        "flow predicate callers should route predicate payload application through the flow query boundary"
    );
    assert!(
        !predicate_fn.contains("TypeGuard::Predicate{")
            && !predicate_fn.contains("&TypeGuard::Truthy")
            && !compact_call.contains("letproperty_guard=TypeGuard::Predicate{"),
        "flow predicate callers should not construct solver predicate payloads locally"
    );
}

#[test]
fn instanceof_guard_payload_uses_flow_query_boundary() {
    let narrowing_source = fs::read_to_string("src/flow/control_flow/narrowing.rs")
        .expect("failed to read flow narrowing source");
    let boundary_source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read flow analysis boundary source");
    let compact_narrowing: String = narrowing_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_boundary: String = boundary_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let instanceof_fn = compact_narrowing
        .split("fninstance_type_from_constructor(")
        .next()
        .and_then(|before_constructor| before_constructor.split("fnnarrow_by_instanceof(").nth(1))
        .expect("failed to locate narrow_by_instanceof body");

    assert!(
        compact_boundary.contains("fnnarrow_by_instanceof_target(")
            && compact_boundary.contains("TypeGuard::Predicate{")
            && compact_boundary.contains("TypeGuard::Instanceof("),
        "flow analysis boundary should own instanceof guard payload construction"
    );
    assert!(
        instanceof_fn.contains("flow_query::narrow_by_instanceof_target("),
        "instanceof flow narrowing should route guard payload application through the flow query boundary"
    );
    assert!(
        !instanceof_fn.contains("TypeGuard::Predicate{")
            && !instanceof_fn.contains("TypeGuard::Instanceof("),
        "instanceof flow narrowing should not construct solver guard payloads locally"
    );
}
