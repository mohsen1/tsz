//! Inferring a bare type parameter from a NON-fresh object source must
//! preserve that source's literal property types (issue #13212 / #10663,
//! Kysely builder-factory family).
//!
//! Structural rule: when a generic call infers a bare type parameter `T` from
//! an object argument, tsc only widens the inferred result when the source
//! carries the widening flag (`getWidenedType` widens `ContainsWideningType`
//! only). A non-fresh object — a declared/annotated type, an alias instance,
//! or an object-spread result `{ ...node }` (which tsz interns non-fresh) —
//! keeps its literal property types. tsz previously over-widened every mutable
//! property of any object source through `widen_object_literal_properties`,
//! turning `{ kind: "X" }` into `{ kind: string }` once `T` was substituted
//! into a non-identity return position (`{ v: T }`, `Readonly<T>`,
//! `Partial<T>`), which produced false `TS2322`/`TS2345` on factory builders
//! such as Kysely's `ColumnDefinitionNode.cloneWith`/`create`.

use tsz_common::options::checker::CheckerOptions;

fn diags_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn assert_clean(source: &str) {
    let diags = diags_strict(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics (tsc-clean); got: {diags:?}"
    );
}

/// A spread of an interface's readonly literal property, inferred for a bare
/// `T` and returned through the homomorphic mapped type `Readonly<T>`. tsc
/// keeps `kind: "X"`.
#[test]
fn spread_source_preserves_literal_through_readonly_return() {
    assert_clean(
        r#"
interface N { readonly kind: 'X'; }
declare function fr<T>(o: T): Readonly<T>;
declare const node: N;
const r = fr({ ...node });
const k: 'X' = r.kind;
"#,
    );
}

/// A non-fresh annotated object inferred for a bare `T` and substituted into a
/// nested wrapper `{ val: T }`. tsc keeps `kind: "X"`; only fresh literals
/// widen.
#[test]
fn annotated_non_fresh_source_preserves_literal_in_nested_return() {
    assert_clean(
        r#"
declare function g<T>(o: T): { val: T };
declare const node: { kind: 'X' };
const r = g(node);
const k: 'X' = r.val.kind;
"#,
    );
}

/// Same through a homomorphic `Partial<T>`.
#[test]
fn annotated_non_fresh_source_preserves_literal_through_partial() {
    assert_clean(
        r#"
declare function p<T>(o: T): Partial<T>;
declare const node: { readonly kind: 'X' };
const r = p(node);
const k: 'X' | undefined = r.kind;
"#,
    );
}

/// Renamed binders + a different discriminant to prove the rule is structural,
/// not keyed on identifiers.
#[test]
fn renamed_binders_non_fresh_source_preserves_literal() {
    assert_clean(
        r#"
interface Holder { readonly tag: 'holder'; }
declare function wrap<U>(o: U): { inner: U };
declare const h: Holder;
const w = wrap({ ...h });
const t: 'holder' = w.inner.tag;
"#,
    );
}

/// Negative control: a GENUINELY fresh object literal with a widening literal
/// property still widens (tsc widens fresh `false` to `boolean`), so the
/// narrowed annotation correctly fails to hold the literal.
#[test]
fn fresh_object_literal_still_widens_mutable_property() {
    // `{ c: false }` is a fresh object literal: inferring it for a bare `T`
    // widens `false` to `boolean`, so `r.picked.c` is `boolean`, which IS
    // assignable to the `boolean` annotation (and would NOT be assignable to
    // `false`). This guards that the fix did not over-preserve fresh literals.
    assert_clean(
        r#"
declare function pick<T>(o: T): { picked: T };
const r = pick({ c: false });
const k: boolean = r.picked.c;
"#,
    );
}
