//! An object-literal member written with a computed key must still have its
//! *value* excess-property checked.
//!
//! Structural rule: when an object-literal member's value is itself a fresh
//! object literal, `tsc` runs the excess-property check on that value
//! regardless of how the member's key was written. tsz does the same through
//! the nested excess-property walk, which now matches an element by the name
//! the member's own type carries rather than by the key's syntax.
//!
//! The walk is handed the *type's* member name (`source_prop.name`) and used to
//! match elements with the syntactic resolver, which declines by design for a
//! late-bound computed key — the written text names a variable, not a property.
//! So every computed member was skipped and its value never descended into.
//!
//! Expectations verified against pinned `typescript@7.0.2`
//! (`--noEmit --strict --pretty false --target es2022 --lib es2022`). tsc
//! reports `TS2353` for every key kind below, which is why this is one rule
//! rather than a per-key-kind patch.

use crate::test_utils::check_source_diagnostics;

fn codes(src: &str) -> Vec<u32> {
    let mut out: Vec<u32> = check_source_diagnostics(src)
        .iter()
        .map(|d| d.code)
        .collect();
    out.sort_unstable();
    out
}

fn assert_excess(label: &str, src: &str) {
    let diags = check_source_diagnostics(src);
    let excess: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert_eq!(
        excess.len(),
        1,
        "{label}: expected exactly one TS2353, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    assert!(
        excess[0].message_text.contains("'extra'"),
        "{label}: TS2353 should name 'extra', got: {}",
        excess[0].message_text
    );
}

fn assert_clean(label: &str, src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "{label}: expected no diagnostics, got: {found:?}"
    );
}

const DECLS: &str = r#"
interface Inner { size: number }
interface House { door: Inner; win: Inner }
const K = "door" as const;
enum E { A = "door" }
declare const S: unique symbol;
interface WithSym { [S]: Inner }
interface Numy { 0: Inner }
const KN = 0 as const;
"#;

// ── positive rows: every key kind descends into the member's value ──────────

#[test]
fn plain_key_member_value_reports_excess_property() {
    assert_excess(
        "plain key (control that already passed)",
        &format!(
            "{DECLS}const r: House = {{ door: {{ size: 1, extra: 2 }}, win: {{ size: 1 }} }};"
        ),
    );
}

#[test]
fn computed_const_string_key_member_value_reports_excess_property() {
    assert_excess(
        "late-bound const string key",
        &format!("{DECLS}const r: House = {{ [K]: {{ size: 1, extra: 2 }}, win: {{ size: 1 }} }};"),
    );
}

#[test]
fn computed_numeric_const_key_member_value_reports_excess_property() {
    assert_excess(
        "numeric const key",
        &format!("{DECLS}const r: Numy = {{ [KN]: {{ size: 1, extra: 2 }} }};"),
    );
}

#[test]
fn computed_enum_member_key_member_value_reports_excess_property() {
    assert_excess(
        "enum member key",
        &format!(
            "{DECLS}const r: House = {{ [E.A]: {{ size: 1, extra: 2 }}, win: {{ size: 1 }} }};"
        ),
    );
}

#[test]
fn computed_unique_symbol_key_member_value_reports_excess_property() {
    assert_excess(
        "unique symbol key",
        &format!("{DECLS}const r: WithSym = {{ [S]: {{ size: 1, extra: 2 }} }};"),
    );
}

#[test]
fn syntactic_literal_computed_key_member_value_reports_excess_property() {
    assert_excess(
        "syntactic literal computed key",
        &format!(
            "{DECLS}const r: House = {{ [\"door\"]: {{ size: 1, extra: 2 }}, win: {{ size: 1 }} }};"
        ),
    );
}

#[test]
fn computed_key_member_value_reports_excess_two_levels_deep() {
    assert_excess(
        "two levels deep under a computed key",
        &format!(
            "{DECLS}interface Deep {{ a: {{ b: {{ c: number }} }} }}\n\
             const r: Deep = {{ [\"a\"]: {{ b: {{ c: 1, extra: 2 }} }} }};"
        ),
    );
}

// ── renamed-binder control: the rule is structural, not name-driven ─────────

#[test]
fn renamed_binders_computed_const_string_key_reports_excess_property() {
    assert_excess(
        "renamed binders",
        r#"
interface Cabin { depth: number }
interface Lodge { hatch: Cabin; pane: Cabin }
const Qq = "hatch" as const;
const r: Lodge = { [Qq]: { depth: 1, extra: 2 }, pane: { depth: 1 } };
"#,
    );
}

// ── negative controls: nothing new fires where tsc stays silent ─────────────

#[test]
fn computed_key_member_value_without_excess_stays_clean() {
    assert_clean(
        "no excess property under a computed key",
        &format!("{DECLS}const r: House = {{ [K]: {{ size: 1 }}, win: {{ size: 1 }} }};"),
    );
}

#[test]
fn plain_key_member_value_without_excess_stays_clean() {
    assert_clean(
        "no excess property under a plain key",
        &format!("{DECLS}const r: House = {{ door: {{ size: 1 }}, win: {{ size: 1 }} }};"),
    );
}

#[test]
fn computed_key_whose_expression_is_not_late_bindable_stays_clean() {
    // A widened (non-`const`) key is not late-bound to a single member, so
    // there is no member for the walk to match. tsc's primary code here is
    // TS2322 against the index signature, with the excess text as a chain link
    // rather than a standalone TS2353 — so no new primary code may appear.
    let src = r#"
interface Inner { size: number }
interface Bag { [k: string]: Inner }
let loose = "door";
const r: Bag = { [loose]: { size: 1, extra: 2 } };
"#;
    let found = codes(src);
    assert!(
        !found.contains(&2353),
        "widened computed key must not gain a TS2353, got: {found:?}"
    );
}

#[test]
fn computed_non_const_key_nested_object_literal_reports_missing_property_not_excess() {
    // Carried over from #16571 (the parallel PR fixing the same defect), whose
    // review measured this row on both branches. A `let`-bound key is not
    // late-bound to a member at all, so the member never satisfies `room` and
    // tsc's answer is the *missing-property* code — not an excess one. This
    // pins that the descent does not over-fire into a shape that has no
    // matching member to descend through.
    let src = r#"
let door = "room";
type House = { room: { size: number } };
const built: House = { [door]: { size: 1, extra: 2 } };
"#;
    assert_eq!(codes(src), vec![2741]);
}

#[test]
fn excess_arriving_through_a_variable_is_not_fresh_under_a_computed_key() {
    // Freshness is what licenses the excess check; a value that arrives through
    // a variable is not fresh, and tsc reports nothing. The computed-key path
    // must not manufacture freshness.
    let src = r#"
const door = "room" as const;
type House = { room: { size: number } };
const loose = { size: 1, extra: 2 };
const built: House = { [door]: loose };
"#;
    assert_clean("non-fresh value under a computed key", src);
}

#[test]
fn type_assertion_under_a_computed_key_stays_opaque() {
    // `as` makes the value opaque to excess-property checking; the computed-key
    // path must not make it fresh again.
    let src = r#"
interface Inner { size: number }
interface House { door: Inner }
const K = "door" as const;
const r: House = { [K]: { size: 1, extra: 2 } as Inner };
"#;
    let found = codes(src);
    assert!(
        !found.contains(&2353),
        "asserted value under a computed key must stay opaque, got: {found:?}"
    );
}
