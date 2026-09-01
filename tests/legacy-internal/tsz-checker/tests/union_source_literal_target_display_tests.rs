//! Regression coverage for declared literal-typed *target* display in
//! assignability diagnostics (#12179 diagnostics family — "diagnostics lose
//! root context, stable display, or source span").
//!
//! tsc renders a *declared* target type — an annotation or named type — exactly
//! as written, including its literal property types, at every nesting depth. A
//! target's literal members are only ever widened in a message when the target
//! is itself a *fresh* object literal (which interns a widened canonical shape).
//!
//! tsz previously over-widened: whenever the assignability message went through
//! the union/best-match top-level builder (which fires when the *source* is a
//! union), the declared object target `{ a: 1 }` was text-widened to
//! `{ a: number }`, leaking a type the user never wrote. The fix mirrors the
//! fresh-vs-declared discipline the source side already applies, so a declared
//! object annotation keeps its literal members regardless of the source's shape.
//!
//! Verified against tsc 6.0.2. Binder/property names vary so a hardcoded fix
//! cannot satisfy these.

use crate::test_utils::check_source_diagnostics;

#[track_caller]
fn ts2322(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    ts2322[0].message_text.clone()
}

// 1. Union source assigned to an anonymous object annotation with a numeric
//    literal property: the declared target keeps `{ a: 1; }` (was `{ a: number; }`).
#[test]
fn union_source_keeps_numeric_literal_object_target() {
    let msg = ts2322(
        r#"
type T = { a: 1 } | { a: 2 };
declare const x: T;
const y: { a: 1 } = x;
"#,
    );
    assert!(
        msg.contains("to type '{ a: 1; }'"),
        "expected declared literal target `{{ a: 1; }}`, got: {msg}"
    );
    assert!(
        !msg.contains("a: number"),
        "target literal must not be widened to `number`, got: {msg}"
    );
}

// 2. Multiple literal properties — every declared literal member is preserved.
#[test]
fn union_source_keeps_all_literal_object_members() {
    let msg = ts2322(
        r#"
type Choice = { a: 1 } | { a: 2; b: string };
declare const value: Choice;
const out: { a: 1; d: 3 } = value;
"#,
    );
    assert!(
        msg.contains("to type '{ a: 1; d: 3; }'"),
        "expected `{{ a: 1; d: 3; }}`, got: {msg}"
    );
    assert!(
        !msg.contains("number"),
        "no declared literal member may widen to `number`, got: {msg}"
    );
}

// 3. String-literal property — widening would turn `"x"` into `string`.
#[test]
fn union_source_keeps_string_literal_object_target() {
    let msg = ts2322(
        r#"
type Mode = { kind: "x" } | { kind: "y" };
declare const mode: Mode;
const picked: { kind: "x" } = mode;
"#,
    );
    assert!(
        msg.contains(r#"to type '{ kind: "x"; }'"#),
        "expected `{{ kind: \"x\"; }}`, got: {msg}"
    );
    assert!(
        !msg.contains("kind: string"),
        "string-literal target member must not widen, got: {msg}"
    );
}

// 4. Nested object literal — the declared literal must survive at depth, not
//    just the top level.
#[test]
fn union_source_keeps_nested_literal_object_target() {
    let msg = ts2322(
        r#"
type Wrapped = { a: 1 } | { a: 2 };
declare const w: Wrapped;
const dest: { a: 1; e: { f: 9 } } = w;
"#,
    );
    assert!(
        msg.contains("e: { f: 9; }"),
        "expected nested declared literal `{{ f: 9; }}`, got: {msg}"
    );
    assert!(
        !msg.contains("f: number"),
        "nested literal must not widen, got: {msg}"
    );
}

// 5. Renamed binders / different property spellings — the rule is structural,
//    not keyed on a particular identifier.
#[test]
fn renamed_union_source_keeps_literal_object_target() {
    let msg = ts2322(
        r#"
type Selector = { tag: 10 } | { tag: 20 };
declare const sel: Selector;
const chosen: { tag: 10 } = sel;
"#,
    );
    assert!(
        msg.contains("to type '{ tag: 10; }'") && !msg.contains("tag: number"),
        "expected `{{ tag: 10; }}` with no widening, got: {msg}"
    );
}

// 6. The literal target reached through a type alias is still rendered by name
//    (named declared targets are shown by their alias, never widened).
#[test]
fn named_literal_target_keeps_alias_name() {
    let msg = ts2322(
        r#"
type Source = { a: 1 } | { a: 2 };
interface Target { a: 1 }
declare const s: Source;
const t: Target = s;
"#,
    );
    assert!(
        msg.contains("to type 'Target'") && !msg.contains("a: number"),
        "expected named target `Target` with no widening, got: {msg}"
    );
}

// 7. Negative / unaffected: a concrete (non-union) object source already kept
//    the declared literal target — pin it so the fix stays symmetric.
#[test]
fn concrete_object_source_keeps_literal_object_target() {
    let msg = ts2322(
        r#"
declare const x: { a: 2; b: string };
const y: { a: 1 } = x;
"#,
    );
    assert!(
        msg.contains("to type '{ a: 1; }'") && !msg.contains("a: number"),
        "concrete-source target display must stay `{{ a: 1; }}`, got: {msg}"
    );
}

// 8. Negative / unaffected: a *fresh* object literal SOURCE is still widened for
//    display when assigned to a non-literal-preserving target — the fix only
//    touches the declared-target side and must not disturb source widening.
#[test]
fn fresh_object_literal_source_still_widens_for_nonliteral_target() {
    let msg = ts2322(
        r#"
interface Box { value: boolean }
const b: Box = { value: 1 };
"#,
    );
    // A property-level TS2322 (`number` not assignable to `boolean`) is what tsc
    // reports here; the source side is unchanged by this fix.
    assert!(
        msg.contains("number") && msg.contains("boolean"),
        "fresh-source widening behavior must be unchanged, got: {msg}"
    );
}
