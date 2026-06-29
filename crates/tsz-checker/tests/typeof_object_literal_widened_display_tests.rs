//! `typeof x` over a variable initialized with a (non-`as const`) object
//! literal must render its property types **widened** to their base in
//! diagnostics — `tsc` resolves `typeof x` to `getWidenedType(getTypeOfSymbol(x))`,
//! so `const obj = { x: 1 }` yields `{ x: number; }`, not `{ x: 1; }`.
//!
//! The divergence was display-only: the structural type is already widened on
//! every value / member-access path. The fresh object literal's pre-widened
//! display provenance (the as-written `{ x: 1 }` spelling) was being applied to
//! the *non-fresh* widened query result. Display provenance is only valid for a
//! *fresh* object literal; a regular/widened object must render its canonical
//! shape.
//!
//! Owner: `crates/tsz-solver/src/diagnostics/format/key.rs`. The diagnostic
//! formatter preserves literal display provenance for fresh literals and for
//! generic application arguments, while regular widened `typeof` objects render
//! their canonical shape.

use tsz_checker::test_utils::check_source_code_messages;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .collect()
}

fn assert_target_widened(source: &str, expected_fragment: &str, forbidden_fragment: &str) {
    let messages = ts2322_messages(source);
    assert!(
        messages.iter().any(|m| m.contains(expected_fragment)),
        "expected a TS2322 whose target renders `{expected_fragment}`, got: {messages:#?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains(forbidden_fragment)),
        "no TS2322 should render the un-widened `{forbidden_fragment}`, got: {messages:#?}"
    );
}

#[test]
fn bare_typeof_const_object_literal_widens_property_literals() {
    // `const obj = { x: 1 }` -> `typeof obj` is `{ x: number; }`, not `{ x: 1; }`.
    assert_target_widened(
        r#"
const obj = { x: 1 };
const t: typeof obj = "wrong";
"#,
        "{ x: number; }",
        "{ x: 1; }",
    );
}

#[test]
fn bare_typeof_widens_independent_of_binder_name() {
    // Binder-name variation keeps the rule structural, not name-driven.
    assert_target_widened(
        r#"
const settings = { count: 1 };
const probe: typeof settings = "wrong";
"#,
        "{ count: number; }",
        "{ count: 1; }",
    );
}

#[test]
fn bare_typeof_widens_every_property_in_a_multi_property_object() {
    assert_target_widened(
        r#"
const obj = { x: 1, y: "a" };
const t: typeof obj = "wrong";
"#,
        "{ x: number; y: string; }",
        "{ x: 1; y: \"a\"; }",
    );
}

#[test]
fn bare_typeof_widens_nested_object_properties_recursively() {
    // The nested `{ x: 1 }` widens to `{ x: number; }` too.
    assert_target_widened(
        r#"
const objn = { inner: { x: 1 } };
const t: typeof objn = "wrong";
"#,
        "{ inner: { x: number; }; }",
        "{ inner: { x: 1; }; }",
    );
}

#[test]
fn member_typeof_widens_nested_object_properties() {
    // `typeof objn.inner` is `{ x: number; }` (member-access path).
    assert_target_widened(
        r#"
const objn = { inner: { x: 1 } };
const t: typeof objn.inner = "wrong";
"#,
        "{ x: number; }",
        "{ x: 1; }",
    );
}

#[test]
fn bare_typeof_let_binding_widens_property_literals() {
    // The widening applies to mutable (`let`) bindings too, matching `tsc`.
    assert_target_widened(
        r#"
let lv = { x: 1 };
const t: typeof lv = "wrong";
"#,
        "{ x: number; }",
        "{ x: 1; }",
    );
}

#[test]
fn typeof_as_const_object_preserves_literal_property_types() {
    // Negative control: `as const` preserves the literal surface, so the
    // readonly literal `1` must NOT be widened.
    let messages = ts2322_messages(
        r#"
const c = { x: 1 } as const;
const t: typeof c = "wrong";
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("readonly x: 1")),
        "an `as const` typeof must keep `readonly x: 1`, got: {messages:#?}"
    );
}

#[test]
fn typeof_object_literal_behavior_is_unchanged() {
    // The structural type was already widened; assigning a structurally-valid
    // object stays clean and a wrong nested property still reports `number`.
    let ok = ts2322_messages(
        r#"
const obj = { x: 1 };
const ok: typeof obj = { x: 99 };
"#,
    );
    assert!(
        ok.is_empty(),
        "assigning `{{ x: 99 }}` to `typeof obj` (widened to `{{ x: number }}`) must be clean, got: {ok:#?}"
    );

    let bad = ts2322_messages(
        r#"
const obj = { x: 1 };
const bad: typeof obj = { x: "s" };
"#,
    );
    assert!(
        bad.iter()
            .any(|m| m.contains("'string'") && m.contains("'number'")),
        "a wrong nested property type still reports `number`, got: {bad:#?}"
    );
}
