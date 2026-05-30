//! Tests for `satisfies` / `as` expression spans and their interaction with
//! generic call chains (issue: large-ts-repo parser-2-20).
//!
//! Structural rule: after `parse_non_predicate_type()` returns, the scanner
//! sits on the first token that is NOT part of the type. The `end` field of
//! a `satisfies` or `as` expression node must equal `token_full_start()` — the
//! full start of that next token (matching tsc's `finishNode` default of
//! `scanner.getTokenFullStart()`). Using `token_end()` overshoots and causes
//! node text extraction to include the following token.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{assert_no_errors, assert_span, assert_span_on, parse_source};

// ---------------------------------------------------------------------------
// satisfies expression span — does not overshoot into the following token
// ---------------------------------------------------------------------------

// Three different following-token contexts prove the rule is structural, not
// tied to a specific delimiter.

#[test]
fn satisfies_span_excludes_trailing_semicolon() {
    assert_span(
        "const x = value satisfies string;",
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "value satisfies string",
    );
}

#[test]
fn satisfies_span_excludes_trailing_comma() {
    assert_span(
        "const a = [value satisfies number, 1];",
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "value satisfies number",
    );
}

#[test]
fn satisfies_span_excludes_trailing_close_paren() {
    assert_span(
        "fn(value satisfies boolean)",
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "value satisfies boolean",
    );
}

#[test]
fn satisfies_span_generic_type_rhs_excludes_semicolon() {
    // The `>` closes the type-argument list; the `;` must not be included.
    assert_span(
        "const x = value satisfies Array<number>;",
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "value satisfies Array<number>",
    );
}

#[test]
fn satisfies_span_generic_type_two_args_excludes_semicolon() {
    assert_span(
        "const x = value satisfies Map<string, number>;",
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "value satisfies Map<string, number>",
    );
}

#[test]
fn satisfies_span_nested_generic_type_excludes_semicolon() {
    assert_span(
        "const x = value satisfies ReadonlyArray<Map<string, number>>;",
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "value satisfies ReadonlyArray<Map<string, number>>",
    );
}

// ---------------------------------------------------------------------------
// as expression span — same structural rule
// ---------------------------------------------------------------------------

#[test]
fn as_expression_span_excludes_trailing_semicolon() {
    assert_span(
        "const x = value as string;",
        syntax_kind_ext::AS_EXPRESSION,
        "value as string",
    );
}

#[test]
fn as_expression_span_generic_type_excludes_semicolon() {
    assert_span(
        "const x = value as Array<number>;",
        syntax_kind_ext::AS_EXPRESSION,
        "value as Array<number>",
    );
}

#[test]
fn as_expression_span_generic_two_args_excludes_semicolon() {
    assert_span(
        "const x = value as Map<string, number>;",
        syntax_kind_ext::AS_EXPRESSION,
        "value as Map<string, number>",
    );
}

#[test]
fn as_const_span_excludes_trailing_semicolon() {
    assert_span(
        "const x = value as const;",
        syntax_kind_ext::AS_EXPRESSION,
        "value as const",
    );
}

// ---------------------------------------------------------------------------
// satisfies after a generic call chain — no parse errors
// ---------------------------------------------------------------------------

// Each source uses a different type-parameter spelling to prove the fix is
// structural, not keyed to a single identifier name.

#[test]
fn satisfies_after_generic_call_no_errors() {
    for source in [
        "const x = factory<Item>() satisfies Item[];",
        "const x = factory<Element>() satisfies Element[];",
    ] {
        assert_no_errors(source);
    }
}

#[test]
fn satisfies_after_chained_generic_calls_no_errors() {
    assert_no_errors("const x = builder<K>().configure<V>() satisfies ReadonlyMap<K, V>;");
}

#[test]
fn satisfies_after_deeply_chained_generic_calls_no_errors() {
    assert_no_errors("const x = a<P>().b<Q>().c<R>() satisfies Triple<P, Q, R>;");
}

#[test]
fn satisfies_generic_type_after_non_generic_call_no_errors() {
    assert_no_errors("const x = create() satisfies Map<string, number>;");
}

#[test]
fn satisfies_generic_type_after_member_call_chain_no_errors() {
    assert_no_errors("const x = obj.method<T>().other() satisfies Result<T>;");
}

// Instantiation expressions (f<T> without a following call) are valid TS 4.7+.
#[test]
fn satisfies_after_instantiation_expression_no_errors() {
    assert_no_errors("const x = fn<string> satisfies (() => string);");
}

// ---------------------------------------------------------------------------
// satisfies span correctness after a generic call chain
// ---------------------------------------------------------------------------

#[test]
fn satisfies_after_generic_call_span_correct() {
    let source = "const x = factory<Item>() satisfies Item[];";
    assert_span(
        source,
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "factory<Item>() satisfies Item[]",
    );
}

#[test]
fn satisfies_after_chained_generic_calls_span_correct() {
    let source = "const x = builder<K>().configure<V>() satisfies ReadonlyMap<K, V>;";
    assert_span(
        source,
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "builder<K>().configure<V>() satisfies ReadonlyMap<K, V>",
    );
}

// ---------------------------------------------------------------------------
// chained as / satisfies — both outer and inner spans must not overshoot
// ---------------------------------------------------------------------------

#[test]
fn chained_satisfies_then_as_const_spans_correct() {
    let source = "const x = value satisfies Record<string, number> as const;";
    let (parser, _) = parse_source(source);
    // Outer as-expression wraps the whole chain.
    assert_span_on(
        &parser,
        source,
        syntax_kind_ext::AS_EXPRESSION,
        "value satisfies Record<string, number> as const",
    );
    // Inner satisfies expression must also have the right span.
    assert_span_on(
        &parser,
        source,
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "value satisfies Record<string, number>",
    );
}

// ---------------------------------------------------------------------------
// Recovery: unusual type forms on the RHS must parse without errors
// ---------------------------------------------------------------------------

#[test]
fn satisfies_with_union_type_rhs_no_errors() {
    assert_no_errors("const x = value satisfies string | number;");
}

#[test]
fn satisfies_with_intersection_type_rhs_no_errors() {
    assert_no_errors("const x = value satisfies A & B;");
}

#[test]
fn satisfies_with_function_type_rhs_no_errors() {
    assert_no_errors("const x = value satisfies (x: number) => string;");
}

#[test]
fn satisfies_with_conditional_type_rhs_no_errors() {
    assert_no_errors("const x = value satisfies string extends number ? true : false;");
}

#[test]
fn satisfies_inside_arrow_return_no_errors() {
    assert_no_errors("const f = () => value satisfies string;");
}

#[test]
fn satisfies_in_ternary_consequent_no_errors() {
    assert_no_errors("const x = cond ? value satisfies string : fallback;");
}

#[test]
fn satisfies_in_ternary_alternate_no_errors() {
    assert_no_errors("const x = cond ? other : value satisfies number;");
}
