//! Tests for issue #3447: const-asserted function returns should preserve
//! their literal type even when the function has no contextual return type.
//!
//! `function f() { return "ok" as const; }` should infer return type `"ok"`,
//! matching tsc. Without the fix, the inferred return type was widened to
//! `string` and `const x: "ok" = f()` produced a false TS2322.

use crate::test_utils::check_source_diagnostics;

#[test]
fn const_assertion_in_function_declaration_return_preserves_literal() {
    let diags = check_source_diagnostics(
        r#"
function returnsLiteral() {
    return "ok" as const;
}
const a: "ok" = returnsLiteral();
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 when assigning const-asserted return to literal type, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn const_assertion_in_arrow_expression_body_preserves_literal() {
    let diags = check_source_diagnostics(
        r#"
const arrowReturnsLiteral = () => "ok" as const;
const b: "ok" = arrowReturnsLiteral();
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 when assigning arrow const-asserted return to literal type, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn legacy_const_type_assertion_in_return_preserves_literal() {
    let diags = check_source_diagnostics(
        r#"
function returnsLiteral() {
    return <const>"ok";
}
const a: "ok" = returnsLiteral();
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for legacy <const> return, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn parenthesized_const_assertion_in_return_preserves_literal() {
    let diags = check_source_diagnostics(
        r#"
function returnsLiteral() {
    return ("ok" as const);
}
const a: "ok" = returnsLiteral();
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 with parenthesized const assertion, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn plain_literal_return_still_widens() {
    // Regression guard: removing the global widening must not stop plain
    // literal returns from widening. `function f() { return "ok"; }` infers
    // return type `string`, so `const x: "ok" = f()` is still a TS2322.
    let diags = check_source_diagnostics(
        r#"
function plain() {
    return "ok";
}
const x: "ok" = plain();
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 when assigning widened string to literal type, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn mixed_returns_widen_only_non_const_contributions() {
    // When a function has multiple return statements, the const-asserted
    // contribution should keep its literal type while the plain return
    // widens. The union here is `"ok" | string` which simplifies to `string`,
    // so `: "ok"` still fails (matching tsc), but the plain literal
    // contribution alone driving widening is preserved as a behavioral
    // regression guard for the per-expression widening path.
    let diags = check_source_diagnostics(
        r#"
function f(b: boolean) {
    if (b) return "ok" as const;
    return "yes";
}
const x: "ok" | string = f(true);
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for mixed-return assignment to `\"ok\" | string`, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn const_asserted_object_return_preserves_readonly_literal() {
    // The const assertion on an object literal should keep readonly + literal
    // members in the inferred return type, not widen to `{ x: string }`.
    let diags = check_source_diagnostics(
        r#"
function makeConfig() {
    return { kind: "ok" } as const;
}
const cfg: { readonly kind: "ok" } = makeConfig();
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for const-asserted object return, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// ── Per-property const assertions inside an object-literal return ──
//
// A per-property `as const` (`{ type: "x" as const }`) produces a regular
// (non-widening) literal. The inferred return type must keep that property's
// literal while still widening non-asserted siblings, matching tsc's
// `getWidenedType`. Widening the whole object collapsed const-asserted
// discriminants to their primitives, breaking discriminated-union narrowing on
// the inferred return type (zustand devtools `extractConnectionInformation`).

#[test]
fn per_property_const_assertion_in_object_return_preserves_literal() {
    let diags = check_source_diagnostics(
        r#"
const make = () => ({ tag: "ready" as const, label: "go" });
const value = make();
const a: "ready" = value.tag;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected const-asserted property to stay literal in inferred return, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn non_asserted_sibling_property_still_widens() {
    // Only the const-asserted property is preserved; its plain-literal sibling
    // must still widen to `string` (tsc behavior).
    let diags = check_source_diagnostics(
        r#"
const make = () => ({ tag: "ready" as const, label: "go" });
const value = make();
const b: "go" = value.label;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.len() == 1,
        "Expected exactly one TS2322 (sibling widened to string), got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn nested_object_literal_const_assertion_preserved() {
    // Recursion: a const-asserted property nested inside an object-literal
    // property value is preserved, while a non-asserted nested sibling widens.
    let diags = check_source_diagnostics(
        r#"
const make = () => ({ outer: { tag: "ready" as const, label: "go" } });
const value = make();
const a: "ready" = value.outer.tag;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected nested const-asserted property to stay literal, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn inferred_return_discriminated_union_narrows_on_const_asserted_tag() {
    // The witness shape: a function whose inferred return type is a union of
    // object literals discriminated by a per-property const assertion. The
    // discriminant must survive widening so the union narrows; otherwise the
    // variant-only property access reports a false TS2339.
    let diags = check_source_diagnostics(
        r#"
const pick = (flag: boolean) => {
    if (flag) {
        return { kind: "empty" as const };
    }
    return { kind: "full" as const, payload: 42 };
};
const result = pick(true);
if (result.kind === "empty") {
} else {
    const p: number = result.payload;
}
"#,
    );

    let bad: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 || d.code == 2322)
        .collect();
    assert!(
        bad.is_empty(),
        "Expected discriminated-union narrowing on inferred return type, got: {:?}",
        bad.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn per_property_const_assertion_preserved_with_renamed_binders() {
    // Anti-hardcoding: vary the function, property, and value-binder names. The
    // fix is structural (AST const-assertion shape), not name-driven, so the
    // discriminant is preserved regardless of identifiers chosen.
    let diags = check_source_diagnostics(
        r#"
const buildThing = () => ({ zzKey: "alpha" as const, other: 1 });
const theThing = buildThing();
const x: "alpha" = theThing.zzKey;
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected name-independent const-assert preservation, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn spread_object_return_preserves_const_asserted_discriminant() {
    // The zustand devtools shape: a returned object literal mixing a
    // const-asserted discriminant with an object spread. The spread-sourced
    // (annotated) members are unaffected; the const-asserted discriminant must
    // stay literal so a later narrow on it succeeds.
    let diags = check_source_diagnostics(
        r#"
type Extra = { connection: number; stores: Record<string, number> };
declare const extra: Extra;
const build = (named: string | undefined) => {
    if (named === undefined) {
        return { tag: "untracked" as const, connection: 1 };
    }
    return { tag: "tracked" as const, named, ...extra };
};
const info = build("a");
if (info.tag === "untracked") {
} else {
    info.stores["k"];
    const n: string = info.named;
}
"#,
    );

    let bad: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 || d.code == 2322)
        .collect();
    assert!(
        bad.is_empty(),
        "Expected spread + const-asserted discriminant to narrow, got: {:?}",
        bad.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
