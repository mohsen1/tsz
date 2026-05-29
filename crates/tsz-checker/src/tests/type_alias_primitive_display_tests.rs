//! Regression coverage for type-alias name display in assignability
//! diagnostics (`diagnostics-32-20` family, issue #11611).
//!
//! tsc attaches an `aliasSymbol` (and therefore renders the alias name) only to
//! freshly-constructed structural types — unions, intersections, objects,
//! arrays, tuples, functions, etc. A non-generic type alias whose body resolves
//! to a bare intrinsic keyword or a literal points at a shared singleton type
//! with no alias symbol, so tsc shows the underlying type (`string`, `42`,
//! `never`, …) rather than the alias name — including through alias chains.
//!
//! tsz previously expanded such aliases correctly at top level but leaked the
//! alias name in nested property positions. These tests pin the tsc-compatible
//! behavior for both the expand cases and the keep-the-name cases. The variable
//! names of the aliases are deliberately varied so a hardcoded fix would fail.

use crate::test_utils::check_source_diagnostics;

fn ts2322_message(source: &str) -> String {
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

// 1. Reported repro: a primitive alias used as a nested property type expands
//    to the underlying primitive, not the alias name.
#[test]
fn primitive_alias_expands_in_nested_property() {
    let msg = ts2322_message(
        r#"
type ID = string;
type User = { id: ID; name: string };
const u: User = { id: 1, name: "x" };
"#,
    );
    assert!(
        msg.contains("type 'string'") && !msg.contains("type 'ID'"),
        "expected primitive alias `ID` to render as `string`, got: {msg}"
    );
}

// 2a. Equivalent shape: number-bodied alias, different name (`N`).
#[test]
fn number_alias_expands_in_nested_property() {
    let msg = ts2322_message(
        r#"
type N = number;
type Holder = { value: N };
const h: Holder = { value: "s" };
"#,
    );
    assert!(
        msg.contains("type 'number'") && !msg.contains("type 'N'"),
        "expected number alias `N` to render as `number`, got: {msg}"
    );
}

// 2b. Equivalent shape: literal-bodied alias renders as the literal.
#[test]
fn literal_alias_expands_in_nested_property() {
    let msg = ts2322_message(
        r#"
type Greeting = "hello";
type Box = { tag: Greeting };
const b: Box = { tag: "bye" };
"#,
    );
    assert!(
        msg.contains("type '\"hello\"'") && !msg.contains("type 'Greeting'"),
        "expected literal alias `Greeting` to render as `\"hello\"`, got: {msg}"
    );
}

// 2c. Equivalent shape: `never` keyword alias renders as `never`.
#[test]
fn never_alias_expands_in_nested_property() {
    let msg = ts2322_message(
        r#"
type Empty = never;
type Wrap = { slot: Empty };
const w: Wrap = { slot: 1 };
"#,
    );
    assert!(
        msg.contains("type 'never'") && !msg.contains("type 'Empty'"),
        "expected `never` alias `Empty` to render as `never`, got: {msg}"
    );
}

// 3. Renamed-binding case: the rule must not depend on the alias spelling. A
//    differently-named primitive alias still expands.
#[test]
fn renamed_primitive_alias_still_expands() {
    let msg = ts2322_message(
        r#"
type WhateverNameHere = string;
type Container = { field: WhateverNameHere };
const c: Container = { field: 42 };
"#,
    );
    assert!(
        msg.contains("type 'string'") && !msg.contains("WhateverNameHere"),
        "expected renamed primitive alias to render as `string`, got: {msg}"
    );
}

// 4. Alias-chain case: `A = B; B = string` collapses to `string`.
#[test]
fn primitive_alias_chain_expands_to_underlying() {
    let msg = ts2322_message(
        r#"
type B = string;
type A = B;
type Rec = { k: A };
const r: Rec = { k: 5 };
"#,
    );
    assert!(
        msg.contains("type 'string'") && !msg.contains("type 'A'") && !msg.contains("type 'B'"),
        "expected alias chain `A = B = string` to render as `string`, got: {msg}"
    );
}

// 5a. Negative case: a union-bodied alias KEEPS its name (tsc attaches an alias
//     symbol to union types).
#[test]
fn union_alias_keeps_name_in_nested_property() {
    let msg = ts2322_message(
        r#"
type IdLike = string | number;
type Wrap = { id: IdLike };
const w: Wrap = { id: true };
"#,
    );
    assert!(
        msg.contains("type 'IdLike'"),
        "expected union alias `IdLike` to keep its name, got: {msg}"
    );
}

// 5b. Negative case: an object-bodied alias KEEPS its name.
#[test]
fn object_alias_keeps_name_at_top_level() {
    let msg = ts2322_message(
        r#"
type Point = { x: number; y: number };
const p: Point = 5;
"#,
    );
    assert!(
        msg.contains("type 'Point'"),
        "expected object alias `Point` to keep its name, got: {msg}"
    );
}

// 5c. Negative case: a function-bodied alias KEEPS its name.
#[test]
fn function_alias_keeps_name_in_nested_property() {
    let msg = ts2322_message(
        r#"
type Handler = () => number;
type Wrap = { fn: Handler };
const w: Wrap = { fn: 5 };
"#,
    );
    assert!(
        msg.contains("type 'Handler'"),
        "expected function alias `Handler` to keep its name, got: {msg}"
    );
}
