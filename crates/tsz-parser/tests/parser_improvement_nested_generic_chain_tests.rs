//! Regression coverage for deeply nested generic angle-bracket chains in
//! *type positions* (issue #11322).
//!
//! `parser_improvement_jsx_recovery_tests.rs` already covers the
//! `look_ahead_is_generic_arrow_function` depth tracker (`<T extends …>(…) =>`
//! shapes in `.tsx`) and the ambient-context JSX suppression added in PR #12112.
//! This file covers the *type positions* — type alias RHS, parameter and return
//! annotations, variable annotations, class heritage, mapped types, conditional
//! types with `infer`, type-arguments to call/`new`, and `as`/`satisfies` — that
//! flow through `parse_type_arguments` / `parse_expected_greater_than` and were
//! not exercised by the original PR.

use crate::parser::test_fixture::assert_no_errors;

// ---------------------------------------------------------------------------
// Type alias RHS — the position named in issue #11322.
// ---------------------------------------------------------------------------

#[test]
fn type_alias_nested_awaited_chain() {
    assert_no_errors("type F<T> = Awaited<Promise<Promise<Promise<Promise<T>>>>>;");
}

#[test]
fn type_alias_with_multi_arg_inner_generic() {
    // The inner `Map<string, …>` ends with `>>` — the inner type must absorb
    // one `>` so the outer alias's `>` remains for its own close.
    assert_no_errors("type G<U> = Awaited<Promise<Map<string, Promise<U>>>>;");
}

#[test]
fn type_alias_array_of_array_chain() {
    // Same closing-token shape (`>>>>`) with a non-Promise leaf identifier so
    // the rule isn't anchored to the Awaited/Promise family.
    assert_no_errors("type F<T> = Array<Array<Array<Array<T>>>>;");
}

#[test]
fn type_alias_mixed_utility_chain() {
    assert_no_errors("type F<T> = Partial<Required<Readonly<Pick<T, keyof T>>>>;");
}

// ---------------------------------------------------------------------------
// Function signatures — parameter and return annotations.
// ---------------------------------------------------------------------------

#[test]
fn function_parameter_nested_generic_chain() {
    assert_no_errors("function f<T>(x: Awaited<Promise<Promise<T>>>): void {}");
}

#[test]
fn function_return_type_nested_generic_chain() {
    assert_no_errors("declare function f<T>(x: T): Awaited<Promise<Promise<T>>>;");
}

#[test]
fn function_multiple_nested_parameter_annotations() {
    assert_no_errors("function f<A, B>(x: Map<A, Array<B>>, y: Array<Map<A, B>>): void {}");
}

#[test]
fn arrow_function_with_nested_generic_parameter() {
    assert_no_errors("const f = <T,>(x: Awaited<Promise<Promise<T>>>): T => x as T;");
}

// ---------------------------------------------------------------------------
// Class declarations — heritage clauses and field/method annotations.
// ---------------------------------------------------------------------------

#[test]
fn class_extends_nested_generic_heritage() {
    assert_no_errors(
        "class Base<T> {} class A extends Base<Map<string, Map<string, Map<string, number>>>> {}",
    );
}

#[test]
fn class_method_nested_generic_signature() {
    assert_no_errors(
        "class A<T> { m(x: Map<string, Array<T>>): Map<string, Array<T>> { return x; } }",
    );
}

// ---------------------------------------------------------------------------
// Type assertions, casts, and `satisfies` at depth-4 close (`>>>>`).
//
// `parser_improvement_satisfies_generic_chain_tests.rs` already covers depth-3
// closes; this file pushes one position deeper.
// ---------------------------------------------------------------------------

#[test]
fn as_expression_with_deep_nested_chain() {
    assert_no_errors("const x = value as Array<Array<Array<Array<string>>>>;");
}

#[test]
fn old_style_type_assertion_with_nested_generic() {
    // `<T>expr` form (non-JSX file): the assertion's own `>` must remain after
    // the inner generics' compound `>` closes are absorbed.
    assert_no_errors("const x = <Array<Array<Array<string>>>>value;");
}

#[test]
fn satisfies_with_deep_nested_chain() {
    assert_no_errors("const x = value satisfies Map<string, Map<string, Map<string, number>>>;");
}

// ---------------------------------------------------------------------------
// Conditional types with `infer` and nested utility chains.
// ---------------------------------------------------------------------------

#[test]
fn conditional_type_with_deep_nested_infer() {
    assert_no_errors("type Unwrap<T> = T extends Promise<Promise<Promise<infer U>>> ? U : never;");
}

#[test]
fn nested_conditional_with_nested_utility_chain() {
    // Three-arm nested conditional with the same nested-utility shape inside
    // each branch — exercises the conditional-arm type-position parser path
    // crossing nested `>>>` closes in each arm.
    assert_no_errors(
        "type C<T> = T extends string ? Awaited<Promise<T>> \
         : T extends number ? Awaited<Promise<T>> \
         : Awaited<Promise<T>>;",
    );
}

// ---------------------------------------------------------------------------
// Mapped types with nested utility on the value side. Iteration variable is
// renamed `P` (not `K`) once to prove the rule isn't anchored to a name a
// future printer-driven decision might pattern-match on (CLAUDE.md §25).
// ---------------------------------------------------------------------------

#[test]
fn mapped_type_with_nested_utility_value() {
    assert_no_errors("type Wrap<T> = { [P in keyof T]: Awaited<Promise<Promise<T[P]>>> };");
}

// ---------------------------------------------------------------------------
// Type-argument lists on call and `new` expressions.
// ---------------------------------------------------------------------------

#[test]
fn call_expression_with_nested_generic_type_arguments() {
    assert_no_errors(
        "declare function f<T>(): T; const x = f<Awaited<Promise<Promise<string>>>>();",
    );
}

#[test]
fn new_expression_with_nested_generic_type_arguments() {
    assert_no_errors(
        "declare class C<T> {} const x = new C<Map<string, Array<Promise<number>>>>();",
    );
}
