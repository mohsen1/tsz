//! TS2403 redeclaration baseline for contextually-typed parameters.
//!
//! When a parameter with no type annotation is contextually typed `T` by its
//! enclosing function and then redeclared by an inner `var x: T`, tsc treats the
//! two as identical (the parameter's *contextual* type is the baseline) and emits
//! no TS2403. tsz previously used the parameter's annotation-only type (`any`) as
//! the baseline, producing a false TS2403 (`any` vs `T`). These witnesses cover
//! every syntactic position the enclosing function's contextual type can come
//! from, with a distinct binder name per case so the fix cannot be name-keyed,
//! plus the negative: a genuinely conflicting redeclaration must still report.
//!
//! Oracle: `conformance/expressions/contextualTyping/generatedContextualTyping.ts`
//! (each of these forms appears there with no TS2403 in the tsc 7.0.2 baseline).

use crate::test_utils::check_source_diagnostics;

/// TS2403 diagnostics for `source` as `(start, message)` pairs.
fn ts2403(source: &str) -> Vec<(u32, String)> {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2403)
        .map(|d| (d.start, d.message_text.clone()))
        .collect()
}

const BASE: &str = "class Base { private p = 0; }\n";

#[test]
fn direct_variable_annotation_no_ts2403() {
    let src =
        format!("{BASE}var alpha: (s: Base[]) => any = q1 => {{ var q1: Base[]; return null; }};");
    assert_eq!(ts2403(&src), vec![], "contextual param from var annotation");
}

#[test]
fn return_position_no_ts2403() {
    let src = format!(
        "{BASE}function beta(): (s: Base[]) => any {{ return q2 => {{ var q2: Base[]; return null; }}; }}"
    );
    assert_eq!(
        ts2403(&src),
        vec![],
        "contextual param from return type annotation"
    );
}

#[test]
fn parameter_default_no_ts2403() {
    let src = format!(
        "{BASE}function gamma(p: (s: Base[]) => any = q3 => {{ var q3: Base[]; return null; }}) {{}}"
    );
    assert_eq!(
        ts2403(&src),
        vec![],
        "contextual param from parameter default annotation"
    );
}

#[test]
fn class_member_initializer_no_ts2403() {
    let src = format!(
        "{BASE}class Delta {{ mem: (s: Base[]) => any = q4 => {{ var q4: Base[]; return null; }}; }}"
    );
    assert_eq!(
        ts2403(&src),
        vec![],
        "contextual param from class member annotation"
    );
}

#[test]
fn plain_assignment_no_ts2403() {
    let src = format!(
        "{BASE}var eps: (s: Base[]) => any; eps = q5 => {{ var q5: Base[]; return null; }};"
    );
    assert_eq!(
        ts2403(&src),
        vec![],
        "contextual param from assignment target type"
    );
}

#[test]
fn object_literal_property_no_ts2403() {
    let src = format!(
        "{BASE}var zeta: {{ key: (s: Base[]) => any }} = {{ key: q6 => {{ var q6: Base[]; return null; }} }};"
    );
    assert_eq!(
        ts2403(&src),
        vec![],
        "contextual param from object-literal property annotation"
    );
}

#[test]
fn conditional_branch_no_ts2403() {
    let src = format!(
        "{BASE}var eta: (s: Base[]) => any = true ? q7 => {{ var q7: Base[]; return null; }} : q7b => {{ var q7b: Base[]; return null; }};"
    );
    assert_eq!(
        ts2403(&src),
        vec![],
        "contextual param through conditional wrapper"
    );
}

#[test]
fn callback_argument_no_ts2403() {
    let src = format!(
        "{BASE}function sink(cb: (s: Base[]) => any) {{}}\nsink(q9 => {{ var q9: Base[]; return null; }});"
    );
    assert_eq!(
        ts2403(&src),
        vec![],
        "contextual param from callback argument position"
    );
}

#[test]
fn callback_argument_nonzero_index_no_ts2403() {
    let src = format!(
        "{BASE}function sink2(first: number, cb: (s: Base[]) => any) {{}}\nsink2(0, q10 => {{ var q10: Base[]; return null; }});"
    );
    assert_eq!(
        ts2403(&src),
        vec![],
        "contextual param from callback at non-zero arg index"
    );
}

#[test]
fn overloaded_callback_argument_first_signature_no_ts2403() {
    // Overloaded callee whose first (applicable) signature takes the callback:
    // the redeclaration baseline is drawn from that signature's parameter type,
    // matching tsc. Exotic overloads whose applicable signature is not the first
    // are not reattempted (they leave the baseline unchanged), so this only
    // asserts the first-signature case.
    let src = format!(
        "{BASE}function accept(cb: (s: Base[]) => any): void;\nfunction accept(x: number): void;\nfunction accept(a: any): void {{}}\naccept(q11 => {{ var q11: Base[]; return null; }});"
    );
    assert_eq!(
        ts2403(&src),
        vec![],
        "contextual param from first overload signature"
    );
}

#[test]
fn genuinely_conflicting_redeclaration_still_ts2403() {
    // Negative: the parameter is contextually `Base[]` but the inner var is
    // annotated `string` — a real conflict tsc reports. The baseline must now be
    // the contextual `Base[]` (not `any`), so the message names `Base[]`.
    let src =
        format!("{BASE}var theta: (s: Base[]) => any = q8 => {{ var q8: string; return null; }};");
    let errors = ts2403(&src);
    assert_eq!(
        errors.len(),
        1,
        "genuine type conflict must still report TS2403: {errors:?}"
    );
    assert!(
        errors[0].1.contains("Base[]") && errors[0].1.contains("string"),
        "TS2403 baseline should be the contextual type Base[], not any: {errors:?}"
    );
}
