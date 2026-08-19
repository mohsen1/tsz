//! A FRESH object-literal argument failing a UNION parameter renders its
//! TS2345 head role-based, matching the TS2322 head after the fresh-literal
//! union fold (#17721 residual 1): each source property keeps its literal
//! exactly when the contextual (target) property type carries a literal of the
//! same primitive base, and widens otherwise — tsc renders the checked fresh
//! type (`getWidenedLiteralLikeTypeForContextualType`), so `{ p: 1, q: 8 }`
//! against `{ p: 1; q: 4 } | { p: 2; q: 8 }` shows
//! `Argument of type '{ p: 1; q: 8; }' ...`.
//!
//! Before the fix the argument head widened unconditionally
//! (`widen_argument_type_for_display`), erasing every literal to its primitive
//! base (`{ p: number; q: number; }`).
//!
//! Non-fresh sources (a plain variable, an `as const` value) keep the widened
//! or literal-frozen display of their declared type and the best-member frame —
//! pinned here as negative controls. All expectations were oracled against the
//! pinned typescript 7.0.2 (`scripts/conformance/oracle.sh`, strict-off
//! default), byte-identical output per witness. Binder names vary across cases
//! so the behavior is structural, not keyed to a spelling.

use crate::test_utils::check_source_diagnostics;

fn ts2345_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn mixed_arm_numeric_literals_preserved_in_argument_head() {
    // Each property fits SOME arm (p:1 fits arm one, q:8 fits arm two) but no
    // single arm admits both, so the diagnostic stays at the argument. Both
    // target property slots carry numeric literals -> both source literals kept.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function g(u: U): void;
g({ p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Argument of type '{ p: 1; q: 8; }'")),
        "numeric literal properties should be preserved in the TS2345 head, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("p: number")),
        "numeric literals must not be widened in the TS2345 head, got: {messages:?}"
    );
}

#[test]
fn string_discriminant_and_numeric_literal_preserved() {
    let messages = ts2345_messages(
        r#"
type V = { mode: "a"; level: 1 } | { mode: "b"; level: 2 };
declare function h(v: V): void;
h({ mode: "a", level: 2 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"Argument of type '{ mode: "a"; level: 2; }'"#)),
        "string and numeric literals should both be preserved, got: {messages:?}"
    );
}

#[test]
fn renamed_binders_preserve_literals() {
    let messages = ts2345_messages(
        r#"
type Wq = { zeta: "on"; gamma: 10 } | { zeta: "off"; gamma: 20 };
declare function feed(r2: Wq): void;
feed({ zeta: "on", gamma: 20 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"Argument of type '{ zeta: "on"; gamma: 20; }'"#)),
        "literal preservation must not depend on binder spelling, got: {messages:?}"
    );
}

#[test]
fn boolean_literal_property_preserved() {
    let messages = ts2345_messages(
        r#"
type B = { on: true; n: 1 } | { on: false; n: 2 };
declare function b(x: B): void;
b({ on: true, n: 2 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Argument of type '{ on: true; n: 2; }'")),
        "boolean literal property should be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("on: boolean")),
        "boolean literal must not be widened to `boolean`, got: {messages:?}"
    );
}

#[test]
fn new_expression_argument_head_preserves_literals() {
    // The construct-argument surface shares the call-argument display path.
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare class Box { constructor(u: U); }
new Box({ p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Argument of type '{ p: 1; q: 8; }'")),
        "new-expression argument head should preserve literals, got: {messages:?}"
    );
}

#[test]
fn generic_call_argument_head_preserves_literals() {
    // A concrete union parameter on a generic function still renders the fresh
    // source role-based. (The TARGET half rendering `U` structurally on
    // generic calls is a pre-existing, separate residual — only the source
    // half is pinned here.)
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function id<T>(t: T, u: U): void;
id(0, { p: 1, q: 8 });
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Argument of type '{ p: 1; q: 8; }'")),
        "generic-call argument head should preserve fresh literals, got: {messages:?}"
    );
}

#[test]
fn non_fresh_variable_argument_keeps_widened_head() {
    // A plain variable widens at declaration; its argument display is the
    // widened declared type, and the best-member frame stays (tsc renders
    // `Argument of type '{ p: number; q: number; }' ...`).
    let messages = ts2345_messages(
        r#"
type U = { p: 1; q: 4 } | { p: 2; q: 8 };
declare function g(u: U): void;
const v = { p: 1, q: 8 };
g(v);
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Argument of type '{ p: number; q: number; }'")),
        "non-fresh variable argument must keep the widened head, got: {messages:?}"
    );
}

#[test]
fn as_const_argument_keeps_readonly_literal_head() {
    // `as const` sources are not FRESH: tsc keeps the readonly literal type in
    // the head and the member frame beneath it (oracled byte-identical today).
    let messages = ts2345_messages(
        r#"
type Cfg = { key: "foo"; value: string } | { key: "bar"; value: number };
declare function accept(c: Cfg): void;
const src = { key: "foo", value: 3 } as const;
accept(src);
"#,
    );
    assert!(
        messages.iter().any(
            |m| m.contains(r#"Argument of type '{ readonly key: "foo"; readonly value: 3; }'"#)
        ),
        "as-const argument must keep its readonly literal head, got: {messages:?}"
    );
}
