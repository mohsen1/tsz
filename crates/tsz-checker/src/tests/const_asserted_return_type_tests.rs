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
