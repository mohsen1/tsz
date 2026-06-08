//! Regression locks for the recursive type-alias body validation walk whose
//! per-node `Vec<NodeIndex>` clones were removed for #11617.
//!
//! `CheckerState` holds the AST as a shared `&'a NodeArena` that never mutates
//! during checking, so the child-node lists of tuple / union / intersection /
//! mapped / template-literal / function / `typeof` type nodes outlive the
//! `&mut self` recursion and can be iterated in place instead of cloned at every
//! nesting level. That change is a pure allocation-traffic reduction, so the
//! walk must still:
//!   1. visit every nested child (broken children keep reporting), and
//!   2. not invent or drop diagnostics on well-formed nested types.
//!
//! The matrix below pins both directions — including a renamed-binder variant so
//! the behavior follows type *shape*, not identifier spelling — across the exact
//! arms that lost their clone. It also covers the duplicate-property early-bail
//! added in the same change (a 0/1-member object type can never trip TS2300 /
//! TS2717 / TS2687).

use tsz_checker::test_utils::{check_source_diagnostics as diagnose, diagnostic_count};

// --- Walk completeness: well-formed nested types stay clean ------------------
// (No clone is no excuse to skip a child or double-visit one; any drift here
//  would surface as a spurious diagnostic on a valid alias.)

#[test]
fn nested_tuple_alias_is_clean() {
    // Exercises the TUPLE_TYPE arm at every depth.
    assert_eq!(
        diagnose("type Deep = [string, [number, [boolean, string]]];\n").len(),
        0
    );
}

#[test]
fn union_and_intersection_alias_is_clean() {
    // Exercises the UNION_TYPE / INTERSECTION_TYPE arms.
    assert_eq!(
        diagnose("type U = ({ a: 1 } & { b: 2 }) | string | number;\n").len(),
        0
    );
}

#[test]
fn function_type_alias_with_array_rest_is_clean() {
    // Exercises the FUNCTION_TYPE arm (param walk + rest check) with a valid rest.
    assert_eq!(
        diagnose("type F = (a: string, b: number, ...rest: number[]) => void;\n").len(),
        0
    );
}

#[test]
fn template_literal_alias_is_clean() {
    // Exercises the TEMPLATE_LITERAL_TYPE arm (span walk).
    assert_eq!(diagnose("type Tpl = `a${string}b${number}c`;\n").len(), 0);
}

#[test]
fn mapped_type_alias_is_clean() {
    // Exercises the MAPPED_TYPE arm (member walk).
    assert_eq!(
        diagnose("type M = { [K in \"a\" | \"b\"]: number };\n").len(),
        0
    );
}

#[test]
fn typeof_with_type_arguments_alias_is_clean() {
    // Exercises the TYPE_QUERY arm (type-argument walk + reuse after the loop).
    assert_eq!(
        diagnose("declare function g<T>(x: T): T;\ntype Q = typeof g<string>;\n").len(),
        0
    );
}

// --- Walk reachability: broken nested children still report ------------------
// Each case plants the error one or more levels *inside* a de-cloned child list,
// so a dropped child would silently lose the diagnostic.

#[test]
fn function_type_alias_non_array_rest_reports_ts2370() {
    // Hits the de-cloned `&func_type.parameters.nodes` passed to the rest check.
    let d = diagnose("type F = (a: string, ...rest: number) => void;\n");
    assert!(diagnostic_count(&d, 2370) >= 1, "diags: {d:?}");
}

#[test]
fn function_type_alias_missing_param_type_reports_ts2304() {
    // Hits the param loop over the de-cloned parameter list.
    let d = diagnose("type F = (x: NopeMissingTypeName) => void;\n");
    assert!(diagnostic_count(&d, 2304) >= 1, "diags: {d:?}");
}

#[test]
fn missing_name_inside_tuple_element_reports_ts2304() {
    // Hits the de-cloned tuple element walk.
    let d = diagnose("type T = [NopeMissingTypeName, number];\n");
    assert!(diagnostic_count(&d, 2304) >= 1, "diags: {d:?}");
}

#[test]
fn missing_name_inside_union_constituent_reports_ts2304() {
    // Hits the de-cloned union/intersection constituent walk.
    let d = diagnose("type U = NopeMissingTypeName | number;\n");
    assert!(diagnostic_count(&d, 2304) >= 1, "diags: {d:?}");
}

#[test]
fn missing_name_inside_union_constituent_reports_ts2304_renamed() {
    // Same shape, different spelling: behavior follows structure, not the name.
    let d = diagnose("type RenamedAlias = AlsoMissingButRenamed | boolean;\n");
    assert!(diagnostic_count(&d, 2304) >= 1, "diags: {d:?}");
}

// --- Duplicate-property early bail (< 2 members can never trip 2300/2717/2687)

#[test]
fn duplicate_property_names_still_report_ts2300_and_ts2717() {
    let d = diagnose("type T = { a: string; a: number };\n");
    assert!(diagnostic_count(&d, 2300) >= 1, "diags: {d:?}");
    assert!(diagnostic_count(&d, 2717) >= 1, "diags: {d:?}");
}

#[test]
fn duplicate_property_names_still_report_ts2300_renamed_binder() {
    // Behavior follows shape, not the property/alias spelling.
    let d = diagnose("type Crate = { slot: string; slot: number };\n");
    assert!(diagnostic_count(&d, 2300) >= 1, "diags: {d:?}");
}

#[test]
fn readonly_disagreement_still_reports_ts2687() {
    let d = diagnose("type T = { readonly a: string; a: string };\n");
    assert!(diagnostic_count(&d, 2687) >= 1, "diags: {d:?}");
}

#[test]
fn single_member_object_type_reports_no_duplicate_family() {
    let d = diagnose("type T = { a: string };\n");
    assert_eq!(diagnostic_count(&d, 2300), 0);
    assert_eq!(diagnostic_count(&d, 2717), 0);
    assert_eq!(diagnostic_count(&d, 2687), 0);
}

#[test]
fn empty_object_type_is_clean() {
    assert_eq!(diagnose("type T = {};\n").len(), 0);
}
