//! Union-source TS2322 elaboration must render the failing constituent *as it
//! appears in the union* — an inline `{ ... }` shows its structural shape, not a
//! coincidentally same-shaped display alias reached through the reverse
//! type-to-def lookup; a named reference keeps its name. Regression coverage for
//! the display half of issue #16513 (the constituent *selection* half — naming
//! the first source-order enum — is owned by the solver reorder in #16523 and
//! its `union_source_enum_elaboration_order_tests`; the enum cases here are the
//! integration check that this display path preserves that selection).

use tsz_checker::test_utils::check_source_strict;
use tsz_common::diagnostics::Diagnostic;

/// The nested elaboration lines' text of the first diagnostic with `code`.
fn chain_texts_for(diags: &[Diagnostic], code: u32) -> Vec<String> {
    diags
        .iter()
        .find(|d| d.code == code)
        .map(|d| {
            d.related_information
                .iter()
                .map(|r| r.message_text.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The nested elaboration lines' text of the first TS2322 (assignment mismatch).
fn chain_texts(diags: &[Diagnostic]) -> Vec<String> {
    chain_texts_for(diags, 2322)
}

/// Assert the elaboration names `present` as a failing constituent and (when
/// given) does not name `absent`.
fn assert_names_constituent(texts: &[String], present: &str, absent: Option<&str>) {
    assert!(
        texts.iter().any(|m| m.contains(present)),
        "expected the source-order constituent `{present}`; got {texts:?}"
    );
    if let Some(absent) = absent {
        assert!(
            !texts.iter().any(|m| m.contains(absent)),
            "must not name `{absent}`; got {texts:?}"
        );
    }
}

#[test]
fn anonymous_object_union_member_is_not_named_by_a_shape_matched_alias() {
    // `U` is a *later* type alias whose reduced body coincides structurally
    // with `c1`'s first constituent. tsc names the anonymous constituent
    // `{ m: number; }`, never the unrelated alias `U`.
    let diags = check_source_strict(
        r#"
declare const c1: { m: number } | { m: string };
const y1: boolean = c1;
type U = { m: number } | { m: number };
"#,
    );
    assert_names_constituent(
        &chain_texts(&diags),
        "Type '{ m: number; }' is not assignable to type 'boolean'",
        Some("Type 'U'"),
    );
}

#[test]
fn enum_union_member_elaboration_names_first_declared_constituent() {
    // Every constituent of `E1 | E2` fails against `boolean`; tsc reports the
    // *first* one in source order (`E1`).
    let diags = check_source_strict(
        r#"
enum E1 { A }
enum E2 { A }
declare const e: E1 | E2;
const ee: boolean = e;
"#,
    );
    let texts = chain_texts(&diags);
    assert!(
        texts
            .iter()
            .any(|m| m == "Type 'E1' is not assignable to type 'boolean'."),
        "expected the first source-order constituent `E1`; got {texts:?}"
    );
    assert!(
        !texts
            .iter()
            .any(|m| m == "Type 'E2' is not assignable to type 'boolean'."),
        "elaboration must name `E1`, not the second constituent `E2`; got {texts:?}"
    );
}

#[test]
fn enum_union_member_elaboration_follows_declaration_order_when_reversed() {
    // Same union, enums declared in the opposite order: the first source-order
    // constituent is now `E2`, and that is what must be named.
    let diags = check_source_strict(
        r#"
enum E2 { A }
enum E1 { A }
declare const e: E1 | E2;
const ee: boolean = e;
"#,
    );
    let texts = chain_texts(&diags);
    assert!(
        texts
            .iter()
            .any(|m| m == "Type 'E2' is not assignable to type 'boolean'."),
        "expected the first source-order constituent `E2`; got {texts:?}"
    );
}

#[test]
fn all_object_union_names_first_source_order_constituent() {
    let diags = check_source_strict(
        r#"
declare const e: { m: number } | { n: string };
const ee: boolean = e;
"#,
    );
    assert_names_constituent(
        &chain_texts(&diags),
        "Type '{ m: number; }' is not assignable to type 'boolean'",
        None,
    );
}

#[test]
fn mixed_object_then_enum_union_names_first_source_order_object() {
    // Ground truth (tsc): the object literal is written first, so it is the
    // named constituent even though the enum was declared earlier.
    let diags = check_source_strict(
        r#"
enum E1 { A }
declare const e: { m: number } | E1;
const ee: boolean = e;
"#,
    );
    assert_names_constituent(
        &chain_texts(&diags),
        "Type '{ m: number; }' is not assignable to type 'boolean'",
        None,
    );
}

#[test]
fn anonymous_object_union_names_first_source_order_member_when_interning_reverses_it() {
    // #16965: the leading `preB`/`preA` declarations force `{ b: number }`'s
    // shape to be content-interned *before* `{ a: string }`, so tsz's canonical
    // (ShapeId-keyed) union member order is `{ b } , { a }` — reversed from the
    // written `{ a: string } | { b: number }`. tsc mints a fresh anonymous type
    // per `TypeLiteral`, so it always names the first *written* failing member.
    // The union header already prints source order (from `union_origin`); the
    // nested elaboration must agree.
    let diags = check_source_strict(
        r#"
declare const preB: { b: number };
declare const preA: { a: string };
declare const v: { a: string } | { b: number };
const x: boolean = v;
"#,
    );
    assert_names_constituent(
        &chain_texts(&diags),
        "Type '{ a: string; }' is not assignable to type 'boolean'",
        Some("Type '{ b: number; }' is not assignable to type 'boolean'"),
    );
}

#[test]
fn anonymous_object_union_ts2345_names_first_source_order_member_when_reversed() {
    // Same reversed-interning shape as above, but the failing relation is a
    // TS2345 argument mismatch rather than a TS2322 assignment. Both diagnostics
    // consume the same union-source elaboration, so the fix covers both.
    let diags = check_source_strict(
        r#"
declare const preB: { b: number };
declare const preA: { a: string };
declare const v: { a: string } | { b: number };
declare function f(x: boolean): void;
f(v);
"#,
    );
    assert_names_constituent(
        &chain_texts_for(&diags, 2345),
        "Type '{ a: string; }' is not assignable to type 'boolean'",
        Some("Type '{ b: number; }' is not assignable to type 'boolean'"),
    );
}

#[test]
fn anonymous_object_union_three_members_names_first_source_order_member_when_reversed() {
    // Three anonymous members with pre-declarations that scramble the canonical
    // order: source order `{ a } | { b } | { c }` must still name `{ a }`.
    let diags = check_source_strict(
        r#"
declare const preC: { c: boolean };
declare const preB: { b: number };
declare const preA: { a: string };
declare const v: { a: string } | { b: number } | { c: boolean };
const x: number = v;
"#,
    );
    assert_names_constituent(
        &chain_texts(&diags),
        "Type '{ a: string; }' is not assignable to type 'number'",
        None,
    );
}

// Named-type-vs-inline-anonymous ranking (`{ z: string } | I` / `… | K` with a
// class declared later, where tsc ranks the named `I`/`K` first — regression
// #16980) is a re-ranking of `Lazy` interface/class members by their declaration
// span, which `order_union_members_for_display` performs from the full compiler's
// definition store. The in-process `check_source_strict` harness does not
// populate that span for `Lazy` interface/class members (only enums and inline
// anonymous objects resolve here), so those cases cannot be reproduced through
// this entry point; they are covered by the CLI oracle matrix in PR #16977 and by
// the conformance corpus. The anonymous-object and enum cases below exercise the
// same display-comparator ordering path that the fix routes through.

#[test]
fn named_type_alias_union_members_keep_their_names() {
    // Named references carry an `aliasSymbol`, so they must keep their names —
    // the anonymous-constituent structural display must not fire here.
    let diags = check_source_strict(
        r#"
type Foo = { a: number };
type Bar = { b: string };
declare const x: Foo | Bar;
const c: boolean = x;
"#,
    );
    let texts = chain_texts(&diags);
    assert!(
        texts
            .iter()
            .any(|m| m == "Type 'Foo' is not assignable to type 'boolean'."),
        "expected the named alias `Foo`; got {texts:?}"
    );
}
