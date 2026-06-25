//! Regression for the `nonObjectUnionNestedExcessPropertyCheck`
//! conformance failure: TS2353's diagnostic target should display only
//! the object-like member of a union (e.g. `IProps`), not the full
//! union (`IProps | number`). Primitive members aren't subject to
//! excess-property checking, so including them is noise.

use crate::test_utils::check_source_diagnostics;

#[test]
fn ts2353_strips_primitive_union_member_from_target_display() {
    let diags = check_source_diagnostics(
        r#"
interface IProps {
    iconProp?: string;
}
const propB1: IProps | number = { INVALID_PROP_NAME: 'share', iconProp: 'test' };
"#,
    );

    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert!(
        !ts2353.is_empty(),
        "expected TS2353 excess-property diagnostic; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let msg = &ts2353[0].message_text;
    assert!(
        msg.contains("'IProps'"),
        "TS2353 should display target as 'IProps' (object member only); got: {msg}"
    );
    assert!(
        !msg.contains("IProps | number"),
        "TS2353 should not display the full union 'IProps | number'; got: {msg}"
    );
}

// --- Issue #14832: fresh-literal excess through `?:` / `??` is a single TS2322
// over the union, not a per-branch TS2353 ---
//
// Structural rule: when a fresh object literal flows through `?:` / `??` / `||`
// and the assigned target makes the result a *union* of differing members, tsc
// runs assignability against the target, fails, and reports ONE TS2322 against
// the union type with a nested excess-property elaboration for the first
// offending member. tsz previously walked each branch literal and emitted a
// separate TS2353 per branch. The single-TS2353 shape is correct only when every
// fresh member shares the identical shape (the union collapses to one type).

/// All TS2353 plus TS2322 excess-property diagnostics, with their elaboration
/// (related-information) messages flattened in for substring assertions.
fn excess_shape(src: &str) -> Vec<(u32, String)> {
    check_source_diagnostics(src)
        .into_iter()
        .filter(|d| d.code == 2322 || d.code == 2353)
        .map(|d| {
            let mut text = d.message_text.clone();
            for related in &d.related_information {
                text.push('\n');
                text.push_str(&related.message_text);
            }
            (d.code, text)
        })
        .collect()
}

#[test]
fn ts14832_ternary_differing_branches_single_ts2322_over_union() {
    // One branch carries an excess property, the other does not → the union has
    // two distinct members. tsc: ONE TS2322 over the union with the excess
    // message nested as elaboration.
    let shape = excess_shape(
        "interface I { a: number }\n\
         declare const cond: boolean;\n\
         const i: I = cond ? { a: 1, b: 2 } : { a: 3 };",
    );
    assert_eq!(
        shape.len(),
        1,
        "expected exactly one diagnostic, got: {shape:?}"
    );
    assert_eq!(
        shape[0].0, 2322,
        "expected TS2322 over the union, got: {shape:?}"
    );
    assert!(
        shape[0].1.contains("is not assignable to type 'I'"),
        "TS2322 head should name target 'I', got: {shape:?}"
    );
    assert!(
        shape[0].1.contains(
            "Object literal may only specify known properties, and 'b' does not exist in type 'I'."
        ),
        "TS2322 should carry the excess-property elaboration for 'b', got: {shape:?}"
    );
}

#[test]
fn ts14832_both_branches_differing_excess_single_ts2322() {
    // Both branches carry a *different* excess property → still ONE TS2322
    // (previously tsz emitted TWO TS2353).
    let shape = excess_shape(
        "interface I { a: number }\n\
         declare const cond: boolean;\n\
         const i: I = cond ? { a: 1, a2: 2 } : { a: 3, b: 4 };",
    );
    assert_eq!(
        shape.len(),
        1,
        "expected exactly one diagnostic, got: {shape:?}"
    );
    assert_eq!(shape[0].0, 2322, "expected a single TS2322, got: {shape:?}");
}

#[test]
fn ts14832_nullish_coalescing_nonliteral_lhs_single_ts2322() {
    // `d ?? { excess }` with a non-literal LHS → union `I | { a; b }`. tsc emits
    // ONE TS2322; the source display keeps the nominal `I` member.
    let shape = excess_shape(
        "interface I { a: number }\n\
         declare const d: I | undefined;\n\
         const i: I = d ?? { a: 1, b: 2 };",
    );
    assert_eq!(
        shape.len(),
        1,
        "expected exactly one diagnostic, got: {shape:?}"
    );
    assert_eq!(shape[0].0, 2322, "expected a single TS2322, got: {shape:?}");
    assert!(
        shape[0].1.contains("'I |"),
        "source display should keep the nominal 'I' union member, got: {shape:?}"
    );
}

#[test]
fn ts14832_assignment_expression_ternary_single_ts2322() {
    // The same rule on an assignment expression (not a declaration).
    let shape = excess_shape(
        "interface I { a: number }\n\
         declare const cond: boolean;\n\
         let i: I;\n\
         i = cond ? { a: 1, b: 2 } : { a: 3 };",
    );
    assert_eq!(
        shape.len(),
        1,
        "expected exactly one diagnostic, got: {shape:?}"
    );
    assert_eq!(shape[0].0, 2322, "expected a single TS2322, got: {shape:?}");
}

#[test]
fn ts14832_uniform_excess_branches_stay_single_ts2353() {
    // Control: identical shape in both branches → the union collapses to one
    // object type → tsc (and tsz) report ONE TS2353, unchanged by this fix.
    let shape = excess_shape(
        "interface I { a: number }\n\
         declare const cond: boolean;\n\
         const i: I = cond ? { a: 1, z: 2 } : { a: 1, z: 3 };",
    );
    assert_eq!(
        shape.len(),
        1,
        "expected exactly one diagnostic, got: {shape:?}"
    );
    assert_eq!(
        shape[0].0, 2353,
        "uniform-excess branches stay TS2353, got: {shape:?}"
    );
    assert!(shape[0].1.contains("'z'"), "got: {shape:?}");
}

#[test]
fn ts14832_nested_branch_excess_still_reported() {
    // Regression guard for #9681: the excess is on a *nested* literal that the
    // top-level relation cannot surface, so the per-branch fallback walk must
    // still report it (TS2353 or TS2322).
    let shape = excess_shape(
        "interface I { a: { b: number } }\n\
         declare const c: boolean;\n\
         const v: I = c ? { a: { b: 1, c: 2 } } : { a: { b: 2 } };",
    );
    assert!(
        shape
            .iter()
            .any(|(code, msg)| (*code == 2353 || *code == 2322) && msg.contains("'c'")),
        "nested branch excess 'c' must still be reported, got: {shape:?}"
    );
}

#[test]
fn ts14832_clean_ternary_branches_emit_nothing() {
    // Control: no excess in either branch → no TS2322/TS2353.
    let shape = excess_shape(
        "interface I { a: number }\n\
         declare const cond: boolean;\n\
         const v: I = cond ? { a: 1 } : { a: 2 };",
    );
    assert!(
        shape.is_empty(),
        "clean ternary must not emit excess diagnostics, got: {shape:?}"
    );
}
