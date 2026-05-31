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
use crate::parser::test_fixture::{
    assert_no_errors, assert_span, assert_span_on, count_nodes, last_node_text, parse_source,
};

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

// ---------------------------------------------------------------------------
// ASI: assertion chaining must NOT span line breaks
//
// `as` and `satisfies` do not bind across a line terminator — the binary
// expression loop already enforces this for the first assertion, and the fix
// removes the internal recursive chaining call so subsequent assertions also
// go through that loop.  When chaining fires, the parser adds two nested
// AS_EXPRESSION nodes (inner then outer); when ASI prevents chaining only one
// node exists.  Counting is therefore the reliable discriminator.
// ---------------------------------------------------------------------------

#[test]
fn as_chain_same_line_produces_two_nodes_and_no_errors() {
    // Same-line chaining must still work — two AS_EXPRESSION nodes are created.
    for source in [
        "const x = v as TypeA as TypeB;",
        "const x = v as X as Y;",
        "const x = v as Alpha as Beta;",
    ] {
        let (parser, _) = parse_source(source);
        assert!(
            parser.get_diagnostics().is_empty(),
            "expected no parse errors for {source:?}, got {:?}",
            parser.get_diagnostics()
        );
        let count = count_nodes(&parser, syntax_kind_ext::AS_EXPRESSION);
        assert_eq!(
            count, 2,
            "same-line chaining must produce 2 AS_EXPRESSION nodes for {source:?}, got {count}"
        );
    }
}

#[test]
fn as_does_not_chain_across_line_break() {
    // Without the fix the parser would create a second AS_EXPRESSION wrapping
    // the chain; after the fix exactly one node exists and spans only the first
    // assertion.  Three sources prove the rule is structural:
    // - as/as pair on two lines
    // - as/satisfies pair on two lines (different operator)
    // - as/as/as triple on three lines (more than one following break)
    for (source, expected) in [
        ("const x = v as TypeA\nas TypeB;", "v as TypeA"),
        ("const z = v as TypeA\nsatisfies TypeB;", "v as TypeA"),
        (
            "const x = val as First\nas Second\nas Third;",
            "val as First",
        ),
    ] {
        let (parser, _) = parse_source(source);
        let count = count_nodes(&parser, syntax_kind_ext::AS_EXPRESSION);
        assert_eq!(
            count, 1,
            "ASI must prevent chaining for {source:?}; expected 1 AS_EXPRESSION, got {count}"
        );
        assert_eq!(
            last_node_text(&parser, source, syntax_kind_ext::AS_EXPRESSION),
            Some(expected),
            "AS_EXPRESSION must cover only the first assertion for {source:?}"
        );
    }
}

#[test]
fn satisfies_then_as_does_not_chain_across_line_break() {
    // `v satisfies TypeA` is complete; `as TypeB` on a new line is separate.
    // Without the fix an extra AS_EXPRESSION wrapping the satisfies is created.
    let source = "const y = v satisfies TypeA\nas TypeB;";
    let (parser, _) = parse_source(source);
    let as_count = count_nodes(&parser, syntax_kind_ext::AS_EXPRESSION);
    assert_eq!(
        as_count, 0,
        "ASI must prevent as-chaining after satisfies; expected 0 AS_EXPRESSION, got {as_count}"
    );
    assert_span(
        source,
        syntax_kind_ext::SATISFIES_EXPRESSION,
        "v satisfies TypeA",
    );
}

#[test]
fn satisfies_does_not_chain_across_line_break() {
    // Both `satisfies` keywords on separate lines must not chain.
    // Without the fix an outer SATISFIES_EXPRESSION would wrap the inner one.
    let source = "const w = v satisfies TypeA\nsatisfies TypeB;";
    let (parser, _) = parse_source(source);
    let count = count_nodes(&parser, syntax_kind_ext::SATISFIES_EXPRESSION);
    assert_eq!(
        count, 1,
        "ASI must prevent chaining; expected 1 SATISFIES_EXPRESSION, got {count}"
    );
    assert_eq!(
        last_node_text(&parser, source, syntax_kind_ext::SATISFIES_EXPRESSION),
        Some("v satisfies TypeA"),
        "the single SATISFIES_EXPRESSION must cover only the first assertion"
    );
}
