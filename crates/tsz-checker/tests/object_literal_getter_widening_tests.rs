//! Tests for literal widening in unannotated object-literal getter return types.
//!
//! When an object literal contains a getter whose body returns a literal, the
//! getter's return type must widen the literal the same way a free-standing
//! getter or a regular function would:
//!
//! ```ts
//! function f() {
//!     return { get x() { return 1; } };
//! }
//! // tsc:  () => { readonly x: number; }
//! // tsz (before fix):  () => { readonly x: 1; }
//! ```
//!
//! The bug was a `preserve_literal_types` flag leak across recursive scopes:
//! when an object literal is itself the return expression of a function,
//! `return_expression_type` sets the flag so the obj literal preserves
//! literal property types. But it must NOT inherit into the getter body's
//! own widening decision — getters are function-like nested scopes (the same
//! reason `return_expression_type` already clears the flag when descending
//! into nested `ARROW_FUNCTION` / `FUNCTION_EXPRESSION` return expressions).

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

/// When the inferred getter return type widens correctly, assigning the
/// inferred function shape to a contextual type that expects the literal
/// `2` should produce a TS2345 whose displayed type contains `number`
/// (widened) — not the literal `1`.
#[test]
fn object_literal_getter_returning_number_literal_widens_in_diagnostic() {
    let source = r#"
declare function expect(value: () => { readonly x: 2 }): void;
function f() {
    return { get x() { return 1; } };
}
expect(f);
"#;
    let diags = get_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "expected a TS2345 for assignment of widened return type to literal `2`, got: {diags:#?}"
    );
    let msg = ts2345[0].1.as_str();
    assert!(
        msg.contains("number"),
        "getter literal `1` must widen to `number` in inferred return-type display, got: {msg}"
    );
    assert!(
        !msg.contains("readonly x: 1"),
        "literal `1` must NOT survive declaration-emit widening, got: {msg}"
    );
}

/// Same widening for string literal getters: `get y() { return 'a'; }` must
/// widen to `string`, not preserve `'a'`.
#[test]
fn object_literal_getter_returning_string_literal_widens_in_diagnostic() {
    let source = r#"
declare function expect(value: () => { readonly y: "b" }): void;
function f() {
    return { get y() { return "a"; } };
}
expect(f);
"#;
    let diags = get_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "expected a TS2345 for assignment of widened return type to literal `'b'`, got: {diags:#?}"
    );
    let msg = ts2345[0].1.as_str();
    assert!(
        msg.contains("string"),
        "getter literal `'a'` must widen to `string` in inferred return-type display, got: {msg}"
    );
}

/// `as const` literals in a getter body must STILL be preserved — the fix
/// must not regress the existing contextual-literal preservation path.
#[test]
fn object_literal_getter_as_const_literal_is_preserved() {
    // The contextual return type `'boolean'` (a string literal) tells the
    // body walk to keep the `as const` literal intact; otherwise the
    // diagnostic would not show `'boolean'`.
    let source = r#"
declare function expect(value: { x: "boolean" }): void;
const obj = { get x() { return 'boolean' as const; } };
expect(obj);
"#;
    let diags = get_diagnostics(source);
    // Either the assignment is accepted (no TS2345), or if a diagnostic is
    // emitted the literal `'boolean'` must appear (NOT widened to `string`).
    for (code, msg) in &diags {
        if *code == 2345 {
            assert!(
                !msg.contains("string"),
                "`as const` getter must NOT widen, got: {msg}"
            );
        }
    }
}

/// Getter mixed with non-getter properties: only the getter's literal
/// widens; the explicit `prop: 1` literal property follows the usual
/// object-literal widening (already correct), and methods correctly widen
/// (also already correct). This regression-fence ensures the fix is
/// scoped to getter-return inference and doesn't disturb sibling paths.
#[test]
fn object_literal_getter_widening_does_not_disturb_sibling_paths() {
    let source = r#"
declare function expect(value: () => { readonly x: 2; method(): 2; prop: 2; }): void;
function f() {
    return {
        get x() { return 1; },
        method() { return 1; },
        prop: 1,
    };
}
expect(f);
"#;
    let diags = get_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "expected a TS2345 for mixed property assignment, got: {diags:#?}"
    );
    let msg = ts2345[0].1.as_str();
    // All three property kinds must show widened `number`.
    assert!(
        msg.contains("readonly x: number"),
        "getter property must widen to `readonly x: number`, got: {msg}"
    );
    assert!(
        msg.contains("method(): number"),
        "method must keep widening to `method(): number`, got: {msg}"
    );
    assert!(
        msg.contains("prop: number"),
        "data property must keep widening to `prop: number`, got: {msg}"
    );
}
