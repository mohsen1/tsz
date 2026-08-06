//! A longhand primitive-keyword union source annotation renders structurally,
//! not repainted with a coincidentally-shaped alias name (issue #16610).
//!
//! `tsc` names a union in a diagnostic only when the annotation carried an
//! `aliasSymbol` — i.e. the source referenced the alias by name. A union
//! written inline (`declare const v: string | number | symbol`) has none, so
//! `tsc` renders it by its members. tsz interns one `TypeId` per content and
//! carries alias identity in a global side table plus a reverse type-to-def
//! lookup, so a lib alias (`PropertyKey`) or a user `type` whose body is the
//! same primitive union would otherwise repaint every longhand occurrence.
//!
//! The fix is occurrence-scoped: it keys off the *written annotation node*
//! (a longhand `UNION_TYPE` of primitive keywords) rather than the shared
//! `TypeId`, so a written-through reference (`: Zed`) — a `TYPE_REFERENCE` —
//! still keeps its name, and a union mixing a named reference is untouched.

use crate::test_utils::{
    check_source_with_libs_code_messages, load_default_lib_files, strict_checker_options,
};

/// The single TS2322 message emitted for `source`, or a panic listing what was
/// actually produced. The default lib is loaded so `PropertyKey` (a lib alias)
/// resolves — its absence is exactly what makes the repaint observable.
fn only_ts2322(source: &str) -> String {
    let libs = load_default_lib_files();
    let diags =
        check_source_with_libs_code_messages(source, "test.ts", strict_checker_options(), &libs);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diags:?}"
    );
    ts2322[0].1.clone()
}

// ---------------------------------------------------------------------------
// Repaint rows: a longhand primitive union renders by its members.
// ---------------------------------------------------------------------------

/// The widest-reach row: the lib alias `PropertyKey` (`string | number |
/// symbol`) must not repaint a longhand `string | number | symbol` that never
/// references it.
#[test]
fn longhand_property_key_shaped_union_renders_structurally() {
    let msg = only_ts2322(
        r#"
declare const v: string | number | symbol;
const probe: boolean = v;
"#,
    );
    assert_eq!(
        msg, "Type 'string | number | symbol' is not assignable to type 'boolean'.",
        "a longhand `string | number | symbol` must render structurally, not as `PropertyKey`; got: {msg:?}"
    );
}

/// A user alias of the same primitive union shape, in the same file, also must
/// not repaint the longhand annotation.
#[test]
fn user_alias_does_not_repaint_longhand_primitive_union() {
    let msg = only_ts2322(
        r#"
type Zed = string | number | symbol;
declare const v: string | number | symbol;
const probe: boolean = v;
"#,
    );
    assert_eq!(
        msg, "Type 'string | number | symbol' is not assignable to type 'boolean'.",
        "a same-shape user alias must not repaint the longhand union; got: {msg:?}"
    );
}

/// A two-member primitive union is the same rule.
#[test]
fn longhand_two_member_primitive_union_renders_structurally() {
    let msg = only_ts2322(
        r#"
type Pair = string | number;
declare const v: string | number;
const probe: boolean = v;
"#,
    );
    assert_eq!(
        msg, "Type 'string | number' is not assignable to type 'boolean'.",
        "a longhand `string | number` must render structurally, not as `Pair`; got: {msg:?}"
    );
}

/// Declaration order does not matter: an alias declared AFTER the longhand
/// annotation must still not repaint it.
#[test]
fn alias_declared_after_longhand_does_not_repaint() {
    let msg = only_ts2322(
        r#"
declare const v: string | number;
type Later = string | number;
const probe: boolean = v;
"#,
    );
    assert_eq!(
        msg, "Type 'string | number' is not assignable to type 'boolean'.",
        "an alias declared after the longhand union must not repaint it; got: {msg:?}"
    );
}

/// The rule is not keyed on any particular alias name — a differently-named
/// same-shape alias behaves identically.
#[test]
fn repaint_is_not_alias_name_specific() {
    let msg = only_ts2322(
        r#"
type Wobble = string | number | symbol;
declare const v: string | number | symbol;
const probe: boolean = v;
"#,
    );
    assert_eq!(
        msg, "Type 'string | number | symbol' is not assignable to type 'boolean'.",
        "the structural render must not depend on the alias's name; got: {msg:?}"
    );
}

/// A parenthesized longhand primitive union is still a longhand primitive
/// union.
#[test]
fn parenthesized_longhand_primitive_union_renders_structurally() {
    let msg = only_ts2322(
        r#"
type Zed = string | number | symbol;
declare const v: (string | number | symbol);
const probe: boolean = v;
"#,
    );
    assert_eq!(
        msg, "Type 'string | number | symbol' is not assignable to type 'boolean'.",
        "a parenthesized longhand primitive union must render structurally; got: {msg:?}"
    );
}

// ---------------------------------------------------------------------------
// Controls: cases that MUST keep the established (named / structural) display.
// ---------------------------------------------------------------------------

/// A written-through user alias reference (`: Zed`) carries an `aliasSymbol`,
/// so it keeps its name — the source referenced the alias.
#[test]
fn written_through_user_alias_keeps_its_name() {
    let msg = only_ts2322(
        r#"
type Zed = string | number | symbol;
declare const v: Zed;
const probe: boolean = v;
"#,
    );
    assert_eq!(
        msg, "Type 'Zed' is not assignable to type 'boolean'.",
        "a written-through alias reference keeps its name; got: {msg:?}"
    );
}

/// A written-through lib alias reference (`: PropertyKey`) likewise keeps its
/// name.
#[test]
fn written_through_property_key_keeps_its_name() {
    let msg = only_ts2322(
        r#"
declare const v: PropertyKey;
const probe: boolean = v;
"#,
    );
    assert_eq!(
        msg, "Type 'PropertyKey' is not assignable to type 'boolean'.",
        "a written-through `PropertyKey` reference keeps its name; got: {msg:?}"
    );
}

/// A union mixing a named reference is not a longhand primitive union — the
/// reference member keeps its name.
#[test]
fn mixed_union_with_named_reference_keeps_reference_name() {
    let msg = only_ts2322(
        r#"
interface Foo { a: number }
declare const v: Foo | string;
const probe: boolean = v;
"#,
    );
    assert!(
        msg.contains("Foo"),
        "a union containing a named reference must keep that reference's name; got: {msg:?}"
    );
}

/// The narrow rule does not touch a union whose members are object-literal
/// annotations mixed with a named-referenced index value: the nested alias name
/// must survive the drill-in. (Regression guard: a broad "anonymous composite"
/// classification would erase the nested `B`.)
#[test]
fn union_with_indexed_object_member_keeps_nested_alias_name() {
    let msg = only_ts2322(
        r#"
type B = { z: number };
const control: number | { [k: string]: B } = { z: null };
"#,
    );
    assert_eq!(
        msg, "Type 'null' is not assignable to type 'B'.",
        "the nested index-signature value alias `B` must survive the drill-in; got: {msg:?}"
    );
}
