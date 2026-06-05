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
