//! End-to-end coverage for computed-bodied type-alias display in assignability
//! diagnostics (issue #10799, the rxjs-project distributive-conditional family).
//!
//! tsc attaches an `aliasSymbol` (and so renders the alias name) only to
//! freshly-constructed structural types. A non-generic alias whose declared
//! body is a *reducing operator* — a conditional or an indexed access — loses
//! its name once the operator resolves: tsc renders the underlying structural
//! result instead. Verified against tsc 6.0.2, e.g.
//! `type P = true extends true ? [string, number] : never` elaborates as
//! `[string, number]`, never `P`.
//!
//! These tests pin the "tuple-like" family (tuple / array / function / scalar /
//! primitive union) the issue targets, vary the alias binder names so a
//! hardcoded fix would fail, and guard the object-result cases that keep their
//! name (object results route through a separate reverse-lookup path and are
//! intentionally left displaying the alias name to avoid a name-collision
//! regression).

use crate::test_utils::check_source_diagnostics;

#[track_caller]
fn ts2322_target(source: &str) -> String {
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

// 1. Conditional reducing to a tuple renders the tuple, not the alias name.
#[test]
fn conditional_alias_to_tuple_renders_underlying_tuple() {
    let msg = ts2322_target(
        r#"
type Pair = true extends true ? [string, number] : never;
type Holder = { v: Pair };
const h: Holder = { v: 0 };
"#,
    );
    assert!(
        msg.contains("[string, number]") && !msg.contains("Pair"),
        "expected conditional tuple alias to render as `[string, number]`, got: {msg}"
    );
}

// 2. Renamed binder, array result — proves the rule is structural, not keyed on
//    a specific identifier.
#[test]
fn conditional_alias_to_array_renders_underlying_array() {
    let msg = ts2322_target(
        r#"
type ListOfThings = string extends string ? number[] : never;
type Wrapper = { field: ListOfThings };
const w: Wrapper = { field: 0 };
"#,
    );
    assert!(
        msg.contains("number[]") && !msg.contains("ListOfThings"),
        "expected conditional array alias to render as `number[]`, got: {msg}"
    );
}

// 3. Conditional reducing to a function type renders the signature.
#[test]
fn conditional_alias_to_function_renders_underlying_signature() {
    let msg = ts2322_target(
        r#"
type Cb = true extends true ? () => void : never;
type Box = { handler: Cb };
const b: Box = { handler: 0 };
"#,
    );
    assert!(
        msg.contains("=> void") && !msg.contains("type 'Cb'"),
        "expected conditional function alias to render its signature, got: {msg}"
    );
}

// 4. Indexed-access reducing to a scalar renders the scalar, not the alias name.
#[test]
fn indexed_access_alias_to_scalar_renders_underlying() {
    let msg = ts2322_target(
        r#"
type Picked = { a: "x" }["a"];
type Holder = { slot: Picked };
const h: Holder = { slot: 0 };
"#,
    );
    assert!(
        msg.contains("type '\"x\"'") && !msg.contains("Picked"),
        "expected indexed-access scalar alias to render as `\"x\"`, got: {msg}"
    );
}

// 5. Negative/no-regression case: a conditional reducing to a *union of objects*
//    keeps the alias name (object results route through the reverse-lookup
//    display path; marking them structurally would mis-paint a shared shape).
#[test]
fn conditional_alias_to_object_union_keeps_name() {
    let diags = check_source_diagnostics(
        r#"
type Shape = true extends true ? { a: 1 } | { b: 2 } : never;
type Holder = { v: Shape };
const h: Holder = { v: 0 };
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.iter().any(|d| d.message_text.contains("Shape")),
        "expected object-union conditional alias to keep its name, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// 6. Negative case: a directly-written tuple alias KEEPS its name (it is a
//    freshly-constructed structural type with an alias symbol).
#[test]
fn direct_tuple_alias_keeps_name() {
    let msg = ts2322_target(
        r#"
type DirectPair = [string, number];
type Holder = { v: DirectPair };
const h: Holder = { v: 0 };
"#,
    );
    assert!(
        msg.contains("DirectPair"),
        "expected a directly-written tuple alias to keep its name, got: {msg}"
    );
}

// ── `keyof` reducing-operator family (issue #12179 / #10914) ──────────────
//
// `keyof <anonymous object type literal>` is a reducing operator like an
// indexed access: it resolves to the operand's key set and tsc renders that
// union, dropping any alias name and the `keyof { ... }` spelling. A `keyof`
// over a *named* operand keeps the `keyof Name` form. Binder names vary across
// cases so a hardcoded fix cannot satisfy them.

// 7. A non-generic alias whose body is `keyof { ... }` renders the key union.
#[test]
fn keyof_anonymous_object_literal_alias_renders_key_union() {
    let msg = ts2322_target(
        r#"
type KeyAlias = keyof { alpha: 1; beta: 2 };
type Holder = { v: KeyAlias };
const h: Holder = { v: 0 };
"#,
    );
    assert!(
        msg.contains("\"alpha\" | \"beta\"") && !msg.contains("KeyAlias"),
        "expected `keyof {{ ... }}` alias to render as the key union, got: {msg}"
    );
}

// 8. Renamed binder, three keys — proves the rule is structural, not keyed on
//    a specific identifier or arity.
#[test]
fn keyof_anonymous_object_literal_alias_renamed_binder() {
    let msg = ts2322_target(
        r#"
type ColumnNames = keyof { id: 1; name: 2; createdAt: 3 };
type Wrapper = { field: ColumnNames };
const w: Wrapper = { field: 0 };
"#,
    );
    assert!(
        msg.contains("\"id\" | \"name\" | \"createdAt\"") && !msg.contains("ColumnNames"),
        "expected renamed `keyof {{ ... }}` alias to render as the key union, got: {msg}"
    );
}

// 9. Inline (un-aliased) `keyof { ... }` annotation also renders the key union.
#[test]
fn keyof_anonymous_object_literal_inline_renders_key_union() {
    let msg = ts2322_target(
        r#"
const x: keyof { zebra: 1; quartz: 2 } = 0;
"#,
    );
    assert!(
        msg.contains("\"zebra\" | \"quartz\"") && !msg.contains("keyof {"),
        "expected inline `keyof {{ ... }}` to render as the key union, got: {msg}"
    );
}

// 10. Negative case: `keyof NamedInterface` keeps the `keyof Name` spelling and
//     is never expanded to the literal key union — a named operand is not
//     anonymous, so the operator form is preserved (the TYPE_LITERAL gate does
//     not fire for a named type reference).
#[test]
fn keyof_named_interface_keeps_keyof_spelling() {
    let msg = ts2322_target(
        r#"
interface Registry { red: 1; green: 2 }
type RegistryKeys = keyof Registry;
const h: RegistryKeys = 0;
"#,
    );
    assert!(
        msg.contains("keyof Registry") && !msg.contains("\"red\""),
        "expected `keyof NamedInterface` to keep its operator spelling, got: {msg}"
    );
}
