//! `IndexAccess` over `typeof X` inside an alias body must defer
//! evaluation and produce the same type whether or not the typeof is
//! parenthesized — the two forms differ only by a `PARENTHESIZED_TYPE`
//! wrapper on the object operand.
//!
//! Pre-fix #9787: `get_type_from_indexed_access_type` eagerly committed
//! to the evaluated form of the alias body when the immediate object
//! operand was a bare `TYPE_QUERY` node. During alias body checking the
//! underlying value's type may not yet be registered in the surrounding
//! `TypeEnvironment`, so the eager evaluation collapsed
//! `IndexAccess(typeof x, K)` to `undefined`. The parenthesized variant
//! escaped this by falling through to the deferred return path, leaving
//! identical source-level types resolving to two different `TypeId`s.

use tsz_common::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_diagnostics(source)
}

fn assert_clean(source: &str) {
    let diags = check(source);
    let relevant: Vec<_> = diags.iter().map(|d| (d.code, &d.message_text)).collect();
    assert!(
        relevant.is_empty(),
        "expected no diagnostics, got: {relevant:?}"
    );
}

#[test]
fn typeof_index_access_alias_resolves_object_property_literal() {
    assert_clean(
        r#"
const x = { tags: "a" } as const;
type A = typeof x["tags"];
type B = (typeof x)["tags"];
declare const a: A;
declare const b: B;
const pa: "a" = a;
const pb: "a" = b;
"#,
    );
}

#[test]
fn typeof_index_access_alias_resolves_object_array_property() {
    assert_clean(
        r#"
const x = { tags: ["a"] } as const;
type A = typeof x["tags"];
type B = (typeof x)["tags"];
declare const a: A;
declare const b: B;
const pa: readonly ["a"] = a;
const pb: readonly ["a"] = b;
"#,
    );
}

#[test]
fn typeof_chained_index_access_alias_over_const_tuple_of_object() {
    // Original reproducer from #9787.
    assert_clean(
        r#"
const x = [{ tags: ["a"] }] as const;
type A = typeof x[0]["tags"];
type B = (typeof x)[0]["tags"];
declare const a: A;
declare const b: B;
const pa: readonly ["a"] = a;
const pb: readonly ["a"] = b;
"#,
    );
}

#[test]
fn typeof_index_access_alias_renamed_identifiers_and_keys() {
    // Different identifier and key names — proves the rule is structural,
    // not keyed on any particular spelling.
    assert_clean(
        r#"
const stuff = { items: [1, 2, 3] } as const;
type Items = typeof stuff["items"];
declare const i: Items;
const probe: readonly [1, 2, 3] = i;
"#,
    );
}

#[test]
fn typeof_chained_index_access_alias_deeper_chain() {
    assert_clean(
        r#"
const x = [{ tags: ["a"] }] as const;
type Inner = typeof x[0]["tags"][0];
declare const inner: Inner;
const probe: "a" = inner;
"#,
    );
}

#[test]
fn typeof_index_access_alias_bidirectional_assignability() {
    // Both directions: the alias must accept the literal construction
    // *and* satisfy the literal annotation.
    assert_clean(
        r#"
const x = [{ tags: ["a"] }] as const;
type A = typeof x[0]["tags"];
declare const a: A;
const fwd: readonly ["a"] = a;
const back: A = ["a"] as const;
"#,
    );
}

#[test]
fn typeof_index_access_alias_is_order_insensitive_to_unrelated_aliases() {
    // Pre-fix behavior was sensitive to surrounding declaration order; the
    // structural fix removes that dependency.
    for source in [
        r#"
type Foo = string;
const x = [{ tags: ["a"] }] as const;
type A = typeof x[0]["tags"];
declare const a: A;
const probe: readonly ["a"] = a;
"#,
        r#"
const x = [{ tags: ["a"] }] as const;
type Foo = string;
type A = typeof x[0]["tags"];
declare const a: A;
const probe: readonly ["a"] = a;
"#,
        r#"
const x = [{ tags: ["a"] }] as const;
type A = typeof x[0]["tags"];
type Foo = string;
declare const a: A;
const probe: readonly ["a"] = a;
"#,
    ] {
        assert_clean(source);
    }
}

#[test]
fn typeof_index_access_alias_in_generic_constraint() {
    // `<T extends typeof x[K]>` shares the alias-style lowering path and
    // hit the same bug, breaking calls that should accept literal args.
    assert_clean(
        r#"
const cfg = { kind: "a" } as const;
function f<T extends typeof cfg["kind"]>(arg: T): T { return arg; }
const r = f("a");
const probe: "a" = r;
"#,
    );
}
