//! Integration tests for `ParenthesizedExpression` emit interactions with
//! IIFE-style call expressions and statement-level wrapping.
//!
//! These tests guard against double-wrapping bugs where the source already
//! has explicit parens around a type-asserted Function/Object/Class
//! expression and the emitter accidentally adds a second pair when erasing
//! the type annotation.
//!
//! See `crates/tsz-emitter/src/emitter/expressions/core/helpers.rs`
//! (`emit_parenthesized`) and
//! `crates/tsz-emitter/src/emitter/statements/core.rs`
//! (`emit_expression_statement`, `outer_paren_will_survive_emit`).

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

/// `(<any>function foo() { })()` is an IIFE: the source paren around
/// `<any>function foo() { }` survives emit (it disambiguates the leading
/// `function` keyword from a function declaration). After type erasure we
/// must emit `(function foo() { })();`, NOT `((function foo() { }))();`.
///
/// Before the fix, `paren_leftmost_function_or_object` was set by the
/// expression-statement and consumed by the inner `FunctionExpression`,
/// causing it to self-parenthesize on top of the surviving source paren.
#[test]
fn iife_typeassertion_function_expression_no_double_parens() {
    let source = "(<any>function foo() { })();\n";
    let output = print_es2015(source);
    assert!(
        output.contains("(function foo() { })();"),
        "IIFE should keep one pair of parens; output:\n{output}"
    );
    assert!(
        !output.contains("((function foo()"),
        "IIFE must not double-parenthesize the callee; output:\n{output}"
    );
}

/// Same shape with `as` instead of `<T>` syntax — both are type erasures and
/// should hit the same `emit_parenthesized` path.
#[test]
fn iife_as_expression_function_no_double_parens() {
    let source = "(function foo() { } as any)();\n";
    let output = print_es2015(source);
    assert!(
        !output.contains("((function"),
        "IIFE with `as` cast must not double-parenthesize; output:\n{output}"
    );
}

/// `(<any>{a:0});` at statement position: the source paren around the type-
/// asserted object literal survives emit (object literals are NOT in
/// `can_strip`). The expression statement must NOT add another wrapping
/// pair, which would produce `(({ a: 0 }));`.
#[test]
fn statement_object_literal_typeassertion_no_double_parens() {
    let source = "(<any>{a:0});\n";
    let output = print_es2015(source);
    assert!(
        output.contains("({ a: 0 });"),
        "Object literal cast at statement position should keep one pair of parens; output:\n{output}"
    );
    assert!(
        !output.contains("(({ a:"),
        "Object literal cast must not double-parenthesize at statement position; output:\n{output}"
    );
}

/// `({ a: 0 } as any);` at statement position — same shape via `as` syntax.
#[test]
fn statement_object_literal_as_no_double_parens() {
    let source = "({ a: 0 } as any);\n";
    let output = print_es2015(source);
    assert!(
        !output.contains("(({"),
        "Object literal `as` cast must not double-parenthesize at statement position; output:\n{output}"
    );
}

/// Naked `(<any>function () { })` (no call) is still an expression statement
/// whose leftmost token is `function`. Here the type-asserted `FunctionExpression`
/// IS in `can_strip` (no enclosing call → both `paren_in_access_position` and
/// `paren_is_direct_call_callee` are false), so the source paren strips. The
/// expression-statement wrap must add its own pair to disambiguate, producing
/// exactly `(function () { });`.
#[test]
fn statement_function_expression_typeassertion_keeps_one_pair() {
    let source = "(<any>function () { });\n";
    let output = print_es2015(source);
    assert!(
        output.contains("(function () { });"),
        "Bare function-expression cast at statement position should keep one pair of parens; output:\n{output}"
    );
    assert!(
        !output.contains("((function"),
        "Bare function-expression cast must not double-parenthesize; output:\n{output}"
    );
}

/// `(<any>(1.0));` is the "nested parenthesized expression, should keep one
/// pair of parenthese" case from
/// `tests/cases/compiler/castExpressionParentheses.ts`. The outer paren
/// survives because the inner expression after type erasure is itself a
/// `ParenthesizedExpression` (so the type-erasure path reuses it). The
/// expression statement must NOT add another wrap.
#[test]
fn statement_nested_paren_typeassertion_keeps_one_pair() {
    let source = "declare var A;\n(<any>(A));\n";
    let output = print_es2015(source);
    assert!(
        output.contains("(A);"),
        "Nested paren cast should keep one pair of parens at statement position; output:\n{output}"
    );
    assert!(
        !output.contains("((A))"),
        "Nested paren cast must not double-parenthesize; output:\n{output}"
    );
}
