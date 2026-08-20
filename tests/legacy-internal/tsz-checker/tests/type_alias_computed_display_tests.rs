//! End-to-end coverage for computed-bodied type-alias display in assignability
//! diagnostics (issue #10799, the rxjs-project distributive-conditional family).
//!
//! tsc attaches an `aliasSymbol` (and so renders the alias name) only to
//! freshly-constructed structural types. A non-generic alias whose declared
//! body is a *reducing operator* — a conditional or an indexed access — loses
//! its name once the operator resolves *to a pre-existing type*: tsc renders
//! the underlying structural result instead. Verified against tsc 6.0.2, e.g.
//! `type P = true extends true ? [string, number] : never` elaborates as
//! `[string, number]`, never `P`.
//!
//! The one carve-out is an **indexed access over a union / `keyof` index**
//! (`type W = T[keyof T]`, `type W = T["a" | "b"]`): tsc builds a *fresh* union
//! via `getUnionType(propTypes, …, aliasSymbol)`, so that union carries the
//! alias symbol and tsc keeps the name (`W`, never `string | number`). A
//! single-key access (`T["a"]`) resolves to one pre-existing member type with
//! no alias symbol and still renders structurally. A non-generic *conditional*
//! never builds such a union — it returns a pre-existing branch type — so even
//! a union-typed branch (`A extends B ? number | boolean : never`) renders
//! structurally as `number | boolean`.
//!
//! These tests pin the tuple / array / function / scalar / primitive-union
//! family, the bare-object family (a conditional / indexed access reducing to an
//! anonymous object renders structurally; a directly-written alias sharing the
//! shape keeps its name via the def store's "direct wins" guard), and the
//! reducing-bodied *application* family (issue #10914: a conditional-bodied
//! alias application such as `DeepReadonly<Config>` drops its alias symbol and
//! renders its resolved structure). Binder names vary so a hardcoded fix fails.

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

// 5b. A conditional reducing to a bare *object* renders the object structurally
//     (issue #10914 / #10799 follow-up; tsc shows `{ a: 1; }`, never the alias
//     name). The shared-shape repaint that previously forced object results to
//     keep the alias name is prevented by the def store's "direct wins" guard.
#[test]
fn conditional_alias_to_object_renders_underlying_object() {
    let msg = ts2322_target(
        r#"
type Boxed = true extends true ? { a: 1 } : never;
type Holder = { v: Boxed };
const h: Holder = { v: 0 };
"#,
    );
    assert!(
        msg.contains("{ a: 1; }") && !msg.contains("Boxed"),
        "expected conditional object alias to render as `{{ a: 1; }}`, got: {msg}"
    );
}

// 5c. Renamed binder, indexed-access reducing to an object — proves the object
//     rule is structural, not keyed on an identifier.
#[test]
fn indexed_access_alias_to_object_renders_underlying_object() {
    let msg = ts2322_target(
        r#"
type Picked = { p: { a: 1 } }["p"];
type Wrapper = { field: Picked };
const w: Wrapper = { field: 0 };
"#,
    );
    assert!(
        msg.contains("{ a: 1; }") && !msg.contains("Picked"),
        "expected indexed-access object alias to render as `{{ a: 1; }}`, got: {msg}"
    );
}

// 5d. Collision guard: when a directly-written alias and a computed alias
//     resolve to the *same* interned object shape, the directly-written alias
//     keeps its name. Without the "direct wins" guard, marking the shared shape
//     computed would strip `Direct`'s name too (the prior `C_uobj` regression).
#[test]
fn computed_object_shape_shared_with_direct_alias_keeps_direct_name() {
    let diags = check_source_diagnostics(
        r#"
type Direct = { a: 1 };
type Computed = true extends true ? { a: 1 } : never;
type Holder = { d: Direct };
const h: Holder = { d: 0 };
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.iter().any(|d| d.message_text.contains("Direct")),
        "expected the directly-written alias to keep its name despite a computed \
         alias sharing the shape, got: {:?}",
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

// ── indexed-access-over-union-index family (fresh union keeps the name) ───
//
// `type W = T[keyof T]` / `type W = T["a" | "b"]` indexes by a union, so tsc
// builds a *fresh* union (`getUnionType(propTypes, …, aliasSymbol)`) that
// carries the alias symbol — the alias name survives (`W`, never the expanded
// `string | number`). This is the inverse of the single-key access in test 4,
// which resolves to one pre-existing member type with no alias symbol. Binder
// names vary so a hardcoded fix cannot satisfy these.

// 6a. Indexed access by `keyof` over a multi-typed object → fresh union → keeps
//     the alias name.
#[test]
fn indexed_access_by_keyof_to_union_keeps_alias_name() {
    let msg = ts2322_target(
        r#"
type Record1 = { a: number; b: string };
type AnyValue = Record1[keyof Record1];
const v: AnyValue = true;
"#,
    );
    assert!(
        msg.contains("'AnyValue'") && !msg.contains("string | number"),
        "expected `T[keyof T]` union alias to keep its name, got: {msg}"
    );
}

// 6b. Renamed binder, explicit union index key → fresh union → keeps the name.
//     Proves the rule is structural (a union *result*), not keyed on `keyof` or
//     any identifier.
#[test]
fn indexed_access_by_explicit_union_key_keeps_alias_name() {
    let msg = ts2322_target(
        r#"
type Palette = { primary: number; secondary: string };
type Swatch = Palette["primary" | "secondary"];
const s: Swatch = true;
"#,
    );
    assert!(
        msg.contains("'Swatch'") && !msg.contains("string | number"),
        "expected `T[\"a\" | \"b\"]` union alias to keep its name, got: {msg}"
    );
}

// 6c. Control: a `keyof` index that collapses to a *single* value type resolves
//     to one member (no fresh union, no alias symbol) and still renders
//     structurally — the carve-out is gated on a union *result*, not on the
//     `keyof` spelling.
#[test]
fn indexed_access_collapsing_to_single_member_renders_underlying() {
    let msg = ts2322_target(
        r#"
type Uniform = { a: number; b: number };
type OnlyValue = Uniform[keyof Uniform];
const v: OnlyValue = "x";
"#,
    );
    assert!(
        msg.contains("type 'number'") && !msg.contains("OnlyValue"),
        "expected a single-member indexed access to render structurally, got: {msg}"
    );
}

// 6d. Control: a non-generic *conditional* whose branch is a primitive union is
//     NOT a fresh-union construction — it returns the pre-existing branch type,
//     so it still renders structurally. Locks the conditional/indexed-access
//     distinction that this carve-out introduced.
#[test]
fn conditional_to_primitive_union_branch_renders_underlying() {
    let msg = ts2322_target(
        r#"
type Branch = string extends string ? number | boolean : never;
type Holder = { v: Branch };
const h: Holder = { v: "x" };
"#,
    );
    assert!(
        msg.contains("number | boolean") && !msg.contains("Branch"),
        "expected a conditional union branch to render structurally, got: {msg}"
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
        msg.contains("\"createdAt\" | \"id\" | \"name\"") && !msg.contains("ColumnNames"),
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
        msg.contains("\"quartz\" | \"zebra\"") && !msg.contains("keyof {"),
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
// ── reducing-bodied alias *application* family (issue #10914) ─────────────
//
// A non-generic alias whose body is an *application* of a generic alias whose
// own declared body is a reducing operator (a conditional or an indexed access)
// drops its alias symbol when the operator resolves: tsc renders the resolved
// structural type, never `Name<Args>` (the application spelling) nor the outer
// alias name. A mapped/object-bodied application (`Partial<T>`) keeps its alias
// symbol. Binder names vary so a hardcoded fix cannot satisfy these.

// 11. A non-generic alias whose body is a conditional-bodied application
//     resolving to an anonymous object renders the resolved object.
#[test]
fn conditional_application_alias_renders_underlying_object() {
    let msg = ts2322_target(
        r#"
type Pick2<T> = T extends object ? { x: 1 } : never;
type RO = Pick2<{ a: 1 }>;
const bad: number = null as any as RO;
"#,
    );
    assert!(
        msg.contains("{ x: 1; }") && !msg.contains("RO") && !msg.contains("Pick2"),
        "expected conditional-bodied application alias to render `{{ x: 1; }}`, got: {msg}"
    );
}

// 12. Renamed binders, a different conditional-bodied utility application
//     resolving to an object — proves the rule is structural, not keyed on a
//     specific identifier such as `DeepReadonly`/`Pick2`.
#[test]
fn renamed_conditional_application_alias_renders_underlying_object() {
    let msg = ts2322_target(
        r#"
type Unwrap<Value> = Value extends object ? { resolved: Value } : Value;
type Final = Unwrap<{ id: 1 }>;
const bad: number = null as any as Final;
"#,
    );
    assert!(
        msg.contains("{ resolved: { id: 1; }; }")
            && !msg.contains("Final")
            && !msg.contains("Unwrap"),
        "expected renamed conditional application alias to render structurally, got: {msg}"
    );
}

// 13. The headline #10914 repro: a recursive `DeepReadonly` application renders
//     the fully-resolved structural object, expanding *every* nested helper
//     application (`DeepReadonly<{ b: number }>` → `{ readonly b: number; }`,
//     `DeepReadonly<string>` → `string`) rather than leaking the internal
//     helper name into the diagnostic.
#[test]
fn recursive_deep_readonly_application_renders_fully_resolved_object() {
    let msg = ts2322_target(
        r#"
type DeepReadonly<T> = T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> } : T;
type Config = { a: { b: number }; c: string };
type RO = DeepReadonly<Config>;
const bad: number = null as any as RO;
"#,
    );
    assert!(
        !msg.contains("DeepReadonly"),
        "expected no internal helper name in the diagnostic, got: {msg}"
    );
    assert!(
        msg.contains("readonly b: number") && msg.contains("readonly c: string"),
        "expected the fully-resolved nested readonly object, got: {msg}"
    );
}

// 14. An *inline* (un-aliased) reducing-bodied application in a property
//     position also renders structurally — covers the nested formatter path.
#[test]
fn inline_conditional_application_renders_underlying_object() {
    let msg = ts2322_target(
        r#"
type Wrap<T> = T extends object ? { wrapped: T } : never;
type Holder = { slot: Wrap<{ a: 1 }> };
const h: Holder = { slot: 0 };
"#,
    );
    assert!(
        msg.contains("{ wrapped: { a: 1; }; }") && !msg.contains("Wrap"),
        "expected inline conditional application to render structurally, got: {msg}"
    );
}

// 15. Negative case: a *mapped*-bodied application (`Partial`-like) keeps its
//     alias symbol and renders the `Name<Args>` application form, since tsc
//     stamps the alias onto a homomorphic mapped result.
#[test]
fn mapped_bodied_application_keeps_application_form() {
    let msg = ts2322_target(
        r#"
type MyPartial<T> = { [P in keyof T]?: T[P] };
type Config = { a: number };
type RO = MyPartial<Config>;
const bad: number = null as any as RO;
"#,
    );
    assert!(
        msg.contains("MyPartial<Config>"),
        "expected a mapped-bodied application to keep its `Name<Args>` form, got: {msg}"
    );
}
