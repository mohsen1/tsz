//! Regression coverage for the object-literal elaboration drill-in gate
//! (issue #15403).
//!
//! `tsc`'s `elaborateElementwise` resolves a drilled-in property type through
//! `getBestMatchIndexedAccessTypeOrUndefined`: indexed access over the whole
//! union, and — when that fails — indexed access into the best-matching union
//! member. For a fresh object-literal source, `findBestTypeForObjectLiteral`
//! picks the FIRST non-array-like member whenever the union contains an
//! array-like member (so a recursive `Json`-style alias resolves the key
//! against a leading primitive, not a trailing index signature). When that best
//! match does not expose the key, `tsc` reports the OUTER whole-object
//! assignment error instead of an inner per-property TS2322.
//!
//! Before this fix tsz drilled into the property (via either the elaboration
//! property resolver or the union index-signature per-property check) and
//! emitted an inner TS2322 at the property value, losing the outer frame.
//!
//! These tests assert the drill-in DECISION (outer whole-object frame vs inner
//! per-property frame) by inspecting the TS2322 source display; the exact outer
//! union chain / recursive-alias headline display is tracked separately.

use crate::test_utils::check_source_strict_messages as check_strict;

/// The single TS2322 message emitted for `source`, or a panic listing what was
/// actually produced.
fn only_ts2322(source: &str) -> String {
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diags:?}"
    );
    ts2322[0].1.clone()
}

// ---------------------------------------------------------------------------
// Best match is a leading primitive: report the OUTER whole-object frame.
// ---------------------------------------------------------------------------

/// Recursive JSON-style alias: `string | ... | Json[] | { [k: string]: Json }`.
/// The union has an array-like member (`Json[]`), so the best match is the
/// leading `string` primitive, which has no property `a`. tsc reports the outer
/// assignment, not an inner `() => number` vs `Json` drill-in.
#[test]
fn fresh_object_vs_recursive_json_union_reports_outer_frame() {
    let msg = only_ts2322(
        r#"
type Json = string | number | boolean | null | Json[] | { [k: string]: Json };
const bad: Json = { a: () => 1 };
"#,
    );
    assert!(
        msg.starts_with("Type '{ a: () => number; }' is not assignable"),
        "expected the OUTER whole-object frame (source `{{ a: () => number; }}`), \
         not an inner property drill-in. Got: {msg:?}"
    );
}

/// Same structural rule with different alias/property identifiers — the gate is
/// structural, not keyed on `Json`/`a` (anti-hardcoding).
#[test]
fn fresh_object_vs_recursive_json_union_reports_outer_frame_alt_names() {
    let msg = only_ts2322(
        r#"
type Payload = string | number | boolean | null | Payload[] | { [key: string]: Payload };
const value: Payload = { handler: () => 1 };
"#,
    );
    assert!(
        msg.starts_with("Type '{ handler: () => number; }' is not assignable"),
        "renamed alias/property must still report the OUTER frame. Got: {msg:?}"
    );
}

/// Non-recursive union with a primitive member AND an array-like member AND an
/// index-signature member: the index-signature per-property check must not fire
/// because the best match is the leading primitive.
#[test]
fn fresh_object_vs_primitive_array_index_union_reports_outer_frame() {
    let msg = only_ts2322(
        r#"
type Cell = string | number | boolean | number[] | { [k: string]: number };
const c: Cell = { a: () => 1 };
"#,
    );
    assert!(
        msg.starts_with("Type '{ a: () => number; }' is not assignable"),
        "a primitive best-match must suppress the index-signature drill-in and \
         report the OUTER frame. Got: {msg:?}"
    );
}

/// Named-property object member (no index signature) with a primitive and an
/// array-like member: best match is still the leading primitive `string`, which
/// lacks `z`, so the outer frame is reported.
#[test]
fn fresh_object_vs_named_member_union_with_array_reports_outer_frame() {
    let msg = only_ts2322(
        r#"
type Elem = { p: number };
type U = string | Elem[] | { z: number };
const u: U = { z: () => 1 };
"#,
    );
    assert!(
        msg.starts_with("Type '{ z: () => number; }' is not assignable"),
        "a leading primitive best-match must report the OUTER frame even when a \
         later named-property member owns the key. Got: {msg:?}"
    );
}

// ---------------------------------------------------------------------------
// Controls: best match exposes the key, OR no array-like member is present —
// the existing inner drill-in must be preserved.
// ---------------------------------------------------------------------------

/// No primitive member: the array member `number[]` doesn't expose `a`, so the
/// best match is `{ a: number }`, which does. tsc drills in — inner TS2322.
#[test]
fn array_plus_object_union_without_primitive_still_drills_in() {
    let msg = only_ts2322(
        r#"
const control: number[] | { a: number } = { a: "x" };
"#,
    );
    assert_eq!(
        msg, "Type 'string' is not assignable to type 'number'.",
        "an array + object union with no primitive member must keep the inner \
         drill-in. Got: {msg:?}"
    );
}

/// Tuple member is array-like; the best match is still `{ a: number }`.
#[test]
fn tuple_plus_object_union_still_drills_in() {
    let msg = only_ts2322(
        r#"
const control: [number] | { a: number } = { a: "x" };
"#,
    );
    assert_eq!(
        msg, "Type 'string' is not assignable to type 'number'.",
        "a tuple + object union must keep the inner drill-in. Got: {msg:?}"
    );
}

/// `readonly` array member is array-like; the best match is `{ a: number }`.
#[test]
fn readonly_array_plus_object_union_still_drills_in() {
    let msg = only_ts2322(
        r#"
const control: readonly string[] | { a: number } = { a: "x" };
"#,
    );
    assert_eq!(
        msg, "Type 'string' is not assignable to type 'number'.",
        "a readonly-array + object union must keep the inner drill-in. Got: {msg:?}"
    );
}

/// No array-like member: the gate does not apply, so the index-signature
/// per-property drill-in is preserved (tsc reports the inner value mismatch).
#[test]
fn union_without_array_member_still_drills_into_index_signature() {
    let msg = only_ts2322(
        r#"
type B = { z: number };
const control: number | { [k: string]: B } = { z: null };
"#,
    );
    assert_eq!(
        msg, "Type 'null' is not assignable to type 'B'.",
        "a union with no array-like member must keep the index-signature \
         drill-in. Got: {msg:?}"
    );
}

/// A single (non-union) index-signature target keeps its per-property drill-in;
/// the array-like-union gate must not disturb the non-union path.
#[test]
fn single_index_signature_target_still_drills_in() {
    let msg = only_ts2322(
        r#"
const control: { [k: string]: number } = { a: () => 1 };
"#,
    );
    assert_eq!(
        msg, "Type '() => number' is not assignable to type 'number'.",
        "a non-union index-signature target must keep the inner drill-in. \
         Got: {msg:?}"
    );
}
