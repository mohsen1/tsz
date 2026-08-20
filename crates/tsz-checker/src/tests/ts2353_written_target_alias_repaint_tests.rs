//! TS2353 target-slot display follows the WRITTEN annotation, not a
//! coincidentally-shaped alias recovered through the reverse type-to-def
//! lookup.
//!
//! tsc renders the type in `'x' does not exist in type 'T'` through the
//! target's `aliasSymbol`: an inline `{ ... }` annotation or union arm has
//! none and renders structurally, while a written reference keeps exactly the
//! name it was written with — even when another alias with an identical body
//! exists in the same file. tsz interns identically-shaped types to one
//! `TypeId`, so before this family landed the display picked an arbitrary
//! same-shaped alias (the earliest registration) for BOTH written forms.
//!
//! Every expectation was oracled against the pinned typescript 7.0.2
//! (`--noEmit --strict`). Binder names vary across cases so the behavior is
//! structural, not keyed to a spelling.

use crate::test_utils::check_source_diagnostics;

fn ts2353_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2353)
        .map(|d| d.message_text)
        .collect()
}

fn assert_single_ts2353_target(source: &str, expected_target: &str) {
    let messages = ts2353_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one TS2353, got: {messages:?}"
    );
    let expected_suffix = format!("does not exist in type '{expected_target}'.");
    assert!(
        messages[0].ends_with(&expected_suffix),
        "TS2353 target mismatch.\n  expected suffix: {expected_suffix}\n  got: {}",
        messages[0]
    );
}

/// Inline union arm; a same-shaped alias declared LATER must not repaint it.
#[test]
fn later_alias_never_repaints_inline_union_arm() {
    assert_single_ts2353_target(
        r#"
type N = { kind: "a"; f(): number } | { kind: "b"; g(): string };
const n: N = { kind: "a", f: () => 1, extra: 2 };
type P = { kind: "a"; f(): number };
"#,
        r#"{ kind: "a"; f(): number; }"#,
    );
}

/// Inline union arm; a same-shaped alias declared EARLIER must not repaint it
/// either — the rule is the written arm syntax, not declaration order.
#[test]
fn earlier_alias_never_repaints_inline_union_arm() {
    assert_single_ts2353_target(
        r#"
type P = { kind: "a"; f(): number };
type N = { kind: "a"; f(): number } | { kind: "b"; g(): string };
const n: N = { kind: "a", f: () => 1, extra: 2 };
"#,
        r#"{ kind: "a"; f(): number; }"#,
    );
}

/// Renamed binders, data-only properties: same structural rule.
#[test]
fn renamed_binders_inline_arm_stays_structural() {
    assert_single_ts2353_target(
        r#"
type Zeta = { tag: "x"; val: boolean } | { tag: "y"; other: symbol };
const z: Zeta = { tag: "x", val: true, oops: 1 };
type Coincide = { tag: "x"; val: boolean };
"#,
        r#"{ tag: "x"; val: boolean; }"#,
    );
}

/// An arm written as an alias REFERENCE keeps the written alias name, even
/// when a different same-shaped alias was declared first.
#[test]
fn reference_arm_keeps_written_alias_over_earlier_same_shaped_alias() {
    assert_single_ts2353_target(
        r#"
type A0 = { kind: "a"; f(): number };
type P = { kind: "a"; f(): number };
type N = P | { kind: "b"; g(): string };
const n: N = { kind: "a", f: () => 1, extra: 2 };
"#,
        "P",
    );
}

/// Direct alias annotation keeps the written alias name over an earlier
/// same-shaped alias.
#[test]
fn direct_annotation_keeps_written_alias_over_earlier_same_shaped_alias() {
    assert_single_ts2353_target(
        r#"
type A0 = { kind: "a"; f(): number };
type P = { kind: "a"; f(): number };
const p: P = { kind: "a", f: () => 1, extra: 2 };
"#,
        "P",
    );
}

/// Inline union ANNOTATION arm renders structurally despite a same-shaped
/// alias in scope.
#[test]
fn inline_union_annotation_arm_stays_structural() {
    assert_single_ts2353_target(
        r#"
type A0 = { kind: "a"; f(): number };
const n: { kind: "a"; f(): number } | { kind: "b"; g(): string } = { kind: "a", f: () => 1, extra: 2 };
"#,
        r#"{ kind: "a"; f(): number; }"#,
    );
}

/// Reference arm inside an inline union annotation keeps the written name.
#[test]
fn reference_arm_in_inline_union_annotation_keeps_written_alias() {
    assert_single_ts2353_target(
        r#"
type A0 = { kind: "a"; f(): number };
type P = { kind: "a"; f(): number };
const n: P | { kind: "b"; g(): string } = { kind: "a", f: () => 1, extra: 2 };
"#,
        "P",
    );
}

/// Non-union inline annotation renders structurally despite a same-shaped
/// alias declared later.
#[test]
fn inline_annotation_stays_structural_over_later_alias() {
    assert_single_ts2353_target(
        r#"
const p: { kind: "a"; f(): number } = { kind: "a", f: () => 1, extra: 2 };
type P = { kind: "a"; f(): number };
"#,
        r#"{ kind: "a"; f(): number; }"#,
    );
}

/// Control: a single alias written directly still prints its own name.
#[test]
fn control_direct_alias_annotation_prints_alias() {
    assert_single_ts2353_target(
        r#"
type P = { kind: "a"; f(): number };
const p: P = { kind: "a", f: () => 1, extra: 2 };
"#,
        "P",
    );
}

/// Control: an alias-referenced arm with no competing alias prints its name.
#[test]
fn control_reference_arm_prints_alias() {
    assert_single_ts2353_target(
        r#"
type P = { kind: "a"; f(): number };
type N = P | { kind: "b"; g(): string };
const n: N = { kind: "a", f: () => 1, extra: 2 };
"#,
        "P",
    );
}

/// Control: no alias anywhere — structural render, unchanged.
#[test]
fn control_no_alias_stays_structural() {
    assert_single_ts2353_target(
        r#"
type N = { kind: "a"; f(): number } | { kind: "b"; g(): string };
const n: N = { kind: "a", f: () => 1, extra: 2 };
"#,
        r#"{ kind: "a"; f(): number; }"#,
    );
}

/// An INTERFACE-referenced arm keeps the interface name (nominal names are
/// never stripped), with a same-shaped alias in scope.
#[test]
fn interface_reference_arm_keeps_interface_name() {
    assert_single_ts2353_target(
        r#"
type A0 = { kind: "a"; f(): number };
interface I { kind: "a"; f(): number }
type N = I | { kind: "b"; g(): string };
const n: N = { kind: "a", f: () => 1, extra: 2 };
"#,
        "I",
    );
}

/// A parenthesized inline arm is still an inline arm.
#[test]
fn parenthesized_inline_arm_stays_structural() {
    assert_single_ts2353_target(
        r#"
type A0 = { tag: "x"; v: number };
type N = ({ tag: "x"; v: number }) | { tag: "y"; w: string };
const n: N = { tag: "x", v: 1, extra: 2 };
"#,
        r#"{ tag: "x"; v: number; }"#,
    );
}

/// Control: a generic alias application keeps its application display; the
/// written-target gate must not fire for reference nodes with type arguments.
#[test]
fn control_generic_alias_application_display_unchanged() {
    assert_single_ts2353_target(
        r#"
type G<T> = { kind: "a"; v: T };
type A0 = { kind: "a"; v: number };
const n: G<number> = { kind: "a", v: 1, extra: 2 };
"#,
        "G<number>",
    );
}

/// RESIDUAL (distinct owner: nested-container property display): the
/// nested-object TS2353 target is the property's type, written inline inside
/// the arm; tsc renders it structurally, tsz still repaints it with a
/// same-shaped alias. Oracled against 7.0.2:
/// `'extra' does not exist in type '{ x: number; }'`.
#[test]
#[ignore = "nested-container property slot still repaints; written-arm gate covers only the annotation's own arms"]
fn nested_property_slot_stays_structural() {
    assert_single_ts2353_target(
        r#"
type Inner = { x: number };
type N = { a: { x: number } } | { b: string };
const n: N = { a: { x: 1, extra: 2 } };
"#,
        r#"{ x: number; }"#,
    );
}

/// RESIDUAL (distinct owner: argument-position annotation resolution): a call
/// argument's TS2353 gets its written target from the parameter declaration,
/// which the site walker does not reach yet. Oracled against 7.0.2:
/// `'extra' does not exist in type '{ kind: "a"; f(): number; }'`.
#[test]
#[ignore = "argument-position sites have no annotation walker; parameter-declaration hop not implemented"]
fn argument_position_inline_arm_stays_structural() {
    assert_single_ts2353_target(
        r#"
type A0 = { kind: "a"; f(): number };
type N = { kind: "a"; f(): number } | { kind: "b"; g(): string };
function take(n: N) {}
take({ kind: "a", f: () => 1, extra: 2 });
"#,
        r#"{ kind: "a"; f(): number; }"#,
    );
}
