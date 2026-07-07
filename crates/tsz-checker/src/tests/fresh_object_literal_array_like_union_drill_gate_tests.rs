//! Drill-in gate for a fresh object literal assigned to a union target that
//! contains an array-like member (`tsc`'s `getBestMatchIndexedAccessTypeOrUndefined`
//! / `findBestTypeForObjectLiteral`).
//!
//! Structural rule: when a fresh object literal is related to a union target
//! that has an array-like member (`T[]`, tuple, or `readonly T[]`) and a
//! property of the literal is only reachable through a member's index
//! signature, `tsc` resolves that property's target against the FIRST
//! non-array-like member in union order — not against any member that happens
//! to expose the key via an index signature. When that best-matching member
//! lacks the key (it is a primitive such as the leading `string` of a recursive
//! JSON alias), `tsc` does not drill into the property; it reports the outer
//! assignment error at the assignment anchor. tsz previously drilled into the
//! property via the index-signature value, emitting an inner
//! `Type '() => number' is not assignable to type 'Json'` at the property span
//! and losing the outer frame.
//!
//! Controls that must keep drilling in (same elaboration family):
//! - unions whose first non-array-like member exposes the key as a named
//!   property (`number[] | { a: number }`, tuple/readonly variants);
//! - unions with no array-like member at all (`number | { [k: string]: B }`),
//!   where the index-signature member is the best match.
//!
//! Property and binder/alias names are varied so the behavior is proven
//! structural, not keyed on a particular identifier spelling.

use crate::test_utils::check_source_diagnostics;

/// Messages for a given assignability code.
fn messages_for(source: &str, code: u32) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| d.message_text)
        .collect()
}

/// True when some emitted diagnostic anchors the inner property-value mismatch
/// (`Type '() => number' is not assignable to type '<member-prop-type>'`).
fn has_inner_function_value_drill(source: &str) -> bool {
    check_source_diagnostics(source).iter().any(|d| {
        d.message_text
            .starts_with("Type '() => number' is not assignable to type")
    })
}

/// True when some emitted diagnostic reports the OUTER object-literal frame
/// (`Type '{ … }' is not assignable to …` / `Argument of type '{ … }' …` /
/// `Type '{ … }' does not satisfy …`).
fn has_outer_object_frame(source: &str) -> bool {
    check_source_diagnostics(source).iter().any(|d| {
        d.message_text.contains("type '{ ")
            || d.message_text.contains("Type '{ ")
            || d.message_text.contains("of type '{ ")
    })
}

// ── Recursive JSON alias: array-like member + leading primitive member ──

#[test]
fn json_var_init_reports_outer_frame_not_inner_property() {
    let src = r#"
type Doc = string | number | boolean | null | Doc[] | { [k: string]: Doc };
const bad: Doc = { field: () => 1 };
"#;
    assert!(
        has_outer_object_frame(src),
        "expected outer object-literal frame, got: {:?}",
        check_source_diagnostics(src)
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        !has_inner_function_value_drill(src),
        "must NOT drill into the property (leading `string` member lacks the key)"
    );
}

#[test]
fn json_assignment_expression_reports_outer_frame() {
    // Assignment-expression context (not a declaration) with a different alias
    // and property name.
    let src = r#"
type Payload = string | number | boolean | null | Payload[] | { [k: string]: Payload };
let sink: Payload;
sink = { entry: () => 1 };
"#;
    assert!(has_outer_object_frame(src));
    assert!(!has_inner_function_value_drill(src));
}

#[test]
fn json_satisfies_reports_outer_frame() {
    let src = r#"
type Tree = string | number | boolean | null | Tree[] | { [k: string]: Tree };
const probe = { leaf: () => 1 } satisfies Tree;
"#;
    // TS1360 "does not satisfy" carries the outer object frame.
    assert!(has_outer_object_frame(src));
    assert!(!has_inner_function_value_drill(src));
}

#[test]
fn json_return_statement_reports_outer_frame() {
    let src = r#"
type Value = string | number | boolean | null | Value[] | { [k: string]: Value };
function make(): Value { return { slot: () => 1 }; }
"#;
    assert!(has_outer_object_frame(src));
    assert!(!has_inner_function_value_drill(src));
}

#[test]
fn json_call_argument_reports_outer_frame() {
    let src = r#"
type Cell = string | number | boolean | null | Cell[] | { [k: string]: Cell };
declare function absorb(c: Cell): void;
absorb({ prop: () => 1 });
"#;
    // Call arguments elaborate to TS2345 with the outer object frame.
    assert!(has_outer_object_frame(src));
    assert!(!has_inner_function_value_drill(src));
}

#[test]
fn json_nested_object_literal_reports_outer_frame() {
    let src = r#"
type Node = string | number | boolean | null | Node[] | { [k: string]: Node };
const deep: Node = { outer: { inner: () => 1 } };
"#;
    assert!(has_outer_object_frame(src));
    assert!(!has_inner_function_value_drill(src));
}

#[test]
fn non_recursive_array_union_with_leading_primitive_reports_outer_frame() {
    // Non-recursive variant proves the rule is about the union shape, not the
    // recursive alias: leading `string` member + `number[]` array member +
    // index-signature member.
    let src = r#"
type Bag = string | number | boolean | number[] | { [k: string]: number };
const bag: Bag = { weight: () => 1 };
"#;
    assert!(has_outer_object_frame(src));
    assert!(!has_inner_function_value_drill(src));
}

// ── Controls: keep drilling into the property ──

#[test]
fn control_array_union_named_member_keeps_inner_drill() {
    // First non-array-like member (`{ a: number }`) exposes the key as a named
    // property, so tsc drills in: `Type '() => number' is not assignable to
    // type 'number'`.
    let src = r#"
type U = number[] | { a: number };
const c: U = { a: () => 1 };
"#;
    let inner = messages_for(src, 2322);
    assert!(
        inner
            .iter()
            .any(|m| m.contains("Type '() => number' is not assignable to type 'number'")),
        "expected inner property drill, got: {inner:?}"
    );
}

#[test]
fn control_tuple_union_named_member_keeps_inner_drill() {
    let src = r#"
type U = [number] | { b: number };
const c: U = { b: () => 1 };
"#;
    let inner = messages_for(src, 2322);
    assert!(
        inner
            .iter()
            .any(|m| m.contains("Type '() => number' is not assignable to type 'number'")),
        "expected inner property drill for tuple union, got: {inner:?}"
    );
}

#[test]
fn control_readonly_array_union_named_member_keeps_inner_drill() {
    let src = r#"
type U = readonly string[] | { c: number };
const c: U = { c: () => 1 };
"#;
    let inner = messages_for(src, 2322);
    assert!(
        inner
            .iter()
            .any(|m| m.contains("Type '() => number' is not assignable to type 'number'")),
        "expected inner property drill for readonly array union, got: {inner:?}"
    );
}

#[test]
fn control_no_array_like_member_keeps_index_signature_drill() {
    // No array-like member: the index-signature member is the best match, so
    // the per-property index-signature value check still drills in.
    let src = r#"
type Elem = { z: number };
type U = number | { [k: string]: Elem };
const c: U = { key: () => 1 };
"#;
    let inner = messages_for(src, 2322);
    assert!(
        inner
            .iter()
            .any(|m| m.contains("Type '() => number' is not assignable to type 'Elem'")),
        "expected inner drill against the index-signature value, got: {inner:?}"
    );
}

// ── Positive control: index-signature-compatible value is accepted ──

#[test]
fn json_index_signature_compatible_value_is_accepted() {
    // A value that IS assignable through the index signature produces no error;
    // the gate must not suppress genuinely-accepted assignments.
    let src = r#"
type Doc = string | number | boolean | null | Doc[] | { [k: string]: Doc };
const ok: Doc = { field: 1 };
"#;
    assert!(
        check_source_diagnostics(src).is_empty(),
        "index-signature-compatible value must be accepted, got: {:?}",
        check_source_diagnostics(src)
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
