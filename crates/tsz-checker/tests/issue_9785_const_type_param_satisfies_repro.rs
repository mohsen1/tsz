//! Regression test for #9785: `const` type parameter over-preserves literal
//! when argument is a `satisfies` expression.
//!
//! ## Structural rule
//!
//! When a generic call has a `const` type parameter and the corresponding
//! argument is a `satisfies E` expression, the inner expression of the
//! satisfies wrapper is contextually typed by `E` (the satisfies target),
//! NOT by the outer call's const-type-parameter contextual type. This is
//! because tsc's `isConstTypeParameterContext` checks the contextual type at
//! the candidate node, and `getContextualType` for a node inside a
//! `SatisfiesExpression` returns the satisfies target — the satisfies wrapper
//! BREAKS the const-type-parameter context for the wrapped inner expression.
//!
//! Concretely, the property widening of `{ a: 1 }` inside
//! `{ a: 1 } satisfies { a: number }` is decided against the contextual
//! `{ a: number }` (satisfies target), under which `1` widens to `number`.
//! The const-type-parameter context on the enclosing call does NOT propagate
//! through the satisfies boundary.
//!
//! This rule is keyed on syntactic structure, not on identifier spellings —
//! renaming the type parameter, the property names, or the satisfies-type
//! alias must not change the decision (see §25 ANTI-HARDCODING DIRECTIVE).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn diags(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn ts2322_diags(source: &str) -> Vec<(u32, String)> {
    diags(source).into_iter().filter(|d| d.0 == 2322).collect()
}

/// Repro from the issue body. With `const T`, the satisfies target widens
/// `1` to `number`, so `r.a` is `number` and the assignment to `5` fails
/// with `Type 'number' is not assignable to type '5'`.
#[test]
fn const_t_with_satisfies_object_widens_property_to_target() {
    let source = r#"
declare function f<const T>(x: T): T;
const r = f({ a: 1 } satisfies { a: number });
const bad: 5 = r.a;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 with widened source `number`, got: {ds:?}",
    );
    assert!(
        ds[0].1.contains("Type 'number'") && ds[0].1.contains("type '5'"),
        "Expected widened `number` in source of TS2322, got: {ds:?}",
    );
}

/// Renamed type parameter / property / literal — the rule must be
/// structural and not name-based. Uses string against primitive `string`
/// to force widening (boolean would not widen because `boolean` is a
/// literal union `true | false` and `isLiteralOfContextualType` matches).
#[test]
fn const_t_with_satisfies_object_widens_renamed() {
    let source = r#"
declare function g<const X>(y: X): X;
const r = g({ name: "alpha" } satisfies { name: string });
const ok: string = r.name;
const bad: "beta" = r.name;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 (assigning `string` to `\"beta\"`), got: {ds:?}",
    );
    assert!(
        ds[0].1.contains("Type 'string'") && ds[0].1.contains("type '\"beta\"'"),
        "Expected widened `string` from satisfies target in TS2322, got: {ds:?}",
    );
}

/// Nested object literal under the satisfies target — inner property is
/// also widened against its inner contextual type.
#[test]
fn const_t_with_satisfies_nested_object_widens_inner() {
    let source = r#"
declare function f<const T>(x: T): T;
const r = f({ a: { b: 1 } } satisfies { a: { b: number } });
const bad: 5 = r.a.b;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 with widened inner `number`, got: {ds:?}",
    );
    assert!(
        ds[0].1.contains("Type 'number'"),
        "Expected widened `number` in source of TS2322, got: {ds:?}",
    );
}

/// Control: non-`const` generic + satisfies — already correct in tsz. Pinned
/// to ensure the fix does not regress this established path.
#[test]
fn non_const_t_with_satisfies_widens_property() {
    let source = r#"
declare function f<T>(x: T): T;
const r = f({ a: 1 } satisfies { a: number });
const bad: 5 = r.a;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 with widened `number`, got: {ds:?}",
    );
    assert!(
        ds[0].1.contains("Type 'number'"),
        "Expected widened `number`, got: {ds:?}",
    );
}

/// Control: `const T` without satisfies — literal preserved. Pinned to
/// ensure the fix does not regress this established path.
#[test]
fn const_t_without_satisfies_preserves_literal() {
    let source = r#"
declare function f<const T>(x: T): T;
const r = f({ a: 1 });
const ok: 1 = r.a;
const bad: 2 = r.a;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 (assigning literal `1` to `2`), got: {ds:?}",
    );
    assert!(
        ds[0].1.contains("Type '1'") && ds[0].1.contains("type '2'"),
        "Expected preserved `1` literal in TS2322, got: {ds:?}",
    );
}

/// When the satisfies target IS the literal, the inner expression's value
/// stays as that literal. The const-context-preservation is moot because
/// the satisfies target itself is literal-preserving.
#[test]
fn const_t_with_satisfies_target_being_literal_keeps_literal() {
    let source = r#"
declare function f<const T>(x: T): T;
const r = f({ a: 1 } satisfies { a: 1 });
const ok: 1 = r.a;
const bad: 2 = r.a;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 (assigning literal `1` to `2`), got: {ds:?}",
    );
    assert!(
        ds[0].1.contains("Type '1'") && ds[0].1.contains("type '2'"),
        "Expected preserved `1` literal (target is literal `1`), got: {ds:?}",
    );
}

/// Inner `as const` nested under the outer satisfies still preserves the
/// inner literal because the inner `as const` re-establishes
/// `in_const_assertion` for its own subtree. The fix must not over-clear.
#[test]
fn const_t_with_satisfies_inner_as_const_preserves_inner() {
    let source = r#"
declare function f<const T>(x: T): T;
const r = f({ a: 1, b: 2 as const } satisfies { a: number; b: 2 });
const a_widened: number = r.a;
const b_preserved: 2 = r.b;
const a_bad: 1 = r.a;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 (a widened to `number`, can't assign to `1`), got: {ds:?}",
    );
    assert!(
        ds[0].1.contains("Type 'number'") && ds[0].1.contains("type '1'"),
        "Expected widened `number` for `a` (the unwrapped property), got: {ds:?}",
    );
}

/// Boolean property: under `boolean` contextual, `true` is preserved
/// because `boolean = true | false` is a literal union, so
/// `isLiteralOfContextualType(true, boolean)` returns true. This test
/// pins that the fix does not over-widen boolean literals — the const
/// type parameter context being broken by the satisfies wrapper still
/// allows the satisfies target's per-literal-kind preservation rule.
#[test]
fn const_t_with_satisfies_boolean_against_boolean_preserved() {
    let source = r#"
declare function f<const T>(x: T): T;
const r = f({ flag: true } satisfies { flag: boolean });
const ok: true = r.flag;
const bad: false = r.flag;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        1,
        "Expected exactly one TS2322 (assigning `true` to `false`), got: {ds:?}",
    );
    assert!(
        ds[0].1.contains("Type 'true'") && ds[0].1.contains("type 'false'"),
        "Expected preserved `true` literal (boolean is literal union), got: {ds:?}",
    );
}
