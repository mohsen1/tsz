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

/// The nested elaboration lines (depth, code, text) of the first TS2322.
fn ts2322_chain(diags: &[Diagnostic]) -> Vec<(u8, u32, String)> {
    diags
        .iter()
        .find(|d| d.code == 2322)
        .map(|d| {
            d.related_information
                .iter()
                .map(|r| (r.depth, r.code, r.message_text.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn chain_texts(diags: &[Diagnostic]) -> Vec<String> {
    ts2322_chain(diags).into_iter().map(|(_, _, m)| m).collect()
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
    let texts = chain_texts(&diags);
    assert!(
        texts
            .iter()
            .any(|m| m.contains("Type '{ m: number; }' is not assignable to type 'boolean'")),
        "expected the anonymous constituent `{{ m: number; }}` in the elaboration; got {texts:?}"
    );
    assert!(
        !texts.iter().any(|m| m.contains("Type 'U'")),
        "elaboration must not name the unrelated alias `U`; got {texts:?}"
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
    let texts = chain_texts(&diags);
    assert!(
        texts
            .iter()
            .any(|m| m.contains("Type '{ m: number; }' is not assignable to type 'boolean'")),
        "expected the first constituent `{{ m: number; }}`; got {texts:?}"
    );
}

#[test]
fn object_union_names_first_source_order_constituent_when_interning_reverses_it() {
    // `preB`/`preA` force `{ b: number }` to be content-interned before
    // `{ a: string }`, reversing the union's canonical (allocation-identity)
    // member order relative to how `v`'s union type was written. tsc mints a
    // fresh anonymous type per `TypeLiteral`, so its relation walk (and its
    // elaboration) still names the first *written* constituent, `{ a: string }`
    // — not `{ b: number }` (issue #16965).
    let diags = check_source_strict(
        r#"
declare const preB: { b: number };
declare const preA: { a: string };
declare const v: { a: string } | { b: number };
const x: boolean = v;
"#,
    );
    let texts = chain_texts(&diags);
    assert!(
        texts
            .iter()
            .any(|m| m.contains("Type '{ a: string; }' is not assignable to type 'boolean'")),
        "expected the first written constituent `{{ a: string; }}`; got {texts:?}"
    );
    assert!(
        !texts.iter().any(|m| m.contains("Type '{ b: number; }'")),
        "elaboration must not name the second written constituent `{{ b: number; }}`; got {texts:?}"
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
    let texts = chain_texts(&diags);
    assert!(
        texts
            .iter()
            .any(|m| m.contains("Type '{ m: number; }' is not assignable to type 'boolean'")),
        "expected the first source-order constituent `{{ m: number; }}`; got {texts:?}"
    );
}

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
