//! Regression tests for #16259: 9 sibling sites shared #16251/#16262's bug shape —
//! `self.parse_expected(SyntaxKind::CloseBraceToken)` advances the scanner past `}`
//! on success, so a `self.token_end()` call *after* it reports the end of the
//! *next* token instead of `}`'s own end. Every node here over-extended its span
//! into whatever token followed its closing brace.
//!
//! Each case is a single-parse-path site (no branching around the `parse_expected`
//! call), so the fix is the same mechanical move as #16262: capture `token_end()`
//! while the current token is still `}`, before `parse_expected` consumes it. The
//! four branching `ClassDeclaration` paths found during this sweep (base
//! `parse_class_declaration`, `parse_class_declaration_with_modifiers`,
//! `parse_declare_class`, `parse_declare_abstract_class`) are NOT covered here —
//! they need per-branch restructuring and are tracked separately.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{assert_span, assert_span_on, parse_source_named};

#[test]
fn jsx_spread_attribute_ends_before_the_next_attribute() {
    // Before the fix, the span extended through the following identifier.
    // JSX requires a `.tsx`/`.jsx` file name to parse `<...>` as JSX.
    let source = "const x = <F {...a} b=\"1\" />;";
    let (parser, _) = parse_source_named("test.tsx", source);
    assert_span_on(
        &parser,
        source,
        syntax_kind_ext::JSX_SPREAD_ATTRIBUTE,
        "{...a}",
    );
}

#[test]
fn jsx_expression_attribute_initializer_ends_before_the_next_attribute() {
    let source = "const x = <F a={x} b=\"1\" />;";
    let (parser, _) = parse_source_named("test.tsx", source);
    assert_span_on(&parser, source, syntax_kind_ext::JSX_EXPRESSION, "{x}");
}

#[test]
fn class_expression_ends_before_the_trailing_variable_declarator() {
    let source = "const C = class { m() {} }, y = 1;";
    assert_span(
        source,
        syntax_kind_ext::CLASS_EXPRESSION,
        "class { m() {} }",
    );
}

#[test]
fn abstract_class_declaration_ends_before_the_next_statement() {
    let source = "abstract class C { m(): void; } let y = 1;";
    assert_span(
        source,
        syntax_kind_ext::CLASS_DECLARATION,
        "abstract class C { m(): void; }",
    );
}

#[test]
fn decorated_class_declaration_ends_before_the_next_statement() {
    let source = "@dec class C { m() {} } let y = 1;";
    assert_span(
        source,
        syntax_kind_ext::CLASS_DECLARATION,
        "@dec class C { m() {} }",
    );
}

#[test]
fn decorated_abstract_class_declaration_ends_before_the_next_statement() {
    let source = "@dec abstract class C { m(): void; } let y = 1;";
    assert_span(
        source,
        syntax_kind_ext::CLASS_DECLARATION,
        "@dec abstract class C { m(): void; }",
    );
}

#[test]
fn class_static_block_ends_before_the_next_member() {
    let source = "class C { static { x; } m() {} }";
    assert_span(
        source,
        syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION,
        "static { x; }",
    );
}

#[test]
fn import_attributes_end_before_the_semicolon() {
    let source = "import x from \"a\" with { type: \"json\" };";
    assert_span(
        source,
        syntax_kind_ext::IMPORT_ATTRIBUTES,
        "with { type: \"json\" }",
    );
}

#[test]
fn switch_statement_ends_before_the_next_statement() {
    // Distinct from its CaseBlock child, which already had the correct end
    // (captured one line earlier in the source); only the outer SwitchStatement
    // node over-extended.
    let source = "switch (a) { case 1: break; } let y = 1;";
    assert_span(
        source,
        syntax_kind_ext::SWITCH_STATEMENT,
        "switch (a) { case 1: break; }",
    );
}

#[test]
fn jsx_spread_attribute_self_closing_tag_ends_before_the_slash() {
    // A second, independently-verified shape: the trailing token here is `/`
    // (self-closing), not an identifier. Before the fix this also overshot,
    // into "{...a} /" — pinning it guards against a fix narrow enough to only
    // handle an identifier as the next token.
    let source = "const x = <F {...a} />;";
    let (parser, _) = parse_source_named("test.tsx", source);
    assert_span_on(
        &parser,
        source,
        syntax_kind_ext::JSX_SPREAD_ATTRIBUTE,
        "{...a}",
    );
}

#[test]
fn two_class_static_blocks_each_end_correctly() {
    // Guards against a fix that only works for the first occurrence.
    let source = "class C { static { a; } static { b; } }";
    let (parser, _) = parse_source_named("test.ts", source);
    for expected in ["static { a; }", "static { b; }"] {
        let arena = parser.get_arena();
        let expected_start = source.find(expected).unwrap();
        let expected_end = expected_start + expected.len();
        let found = arena.nodes.iter().any(|n| {
            n.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
                && n.pos as usize == expected_start
                && n.end as usize == expected_end
        });
        assert!(
            found,
            "no correctly-spanned ClassStaticBlockDeclaration at {expected_start} for {expected:?} in {source:?}: nodes = {:?}",
            arena
                .nodes
                .iter()
                .filter(|n| n.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION)
                .map(|n| (n.pos, n.end))
                .collect::<Vec<_>>()
        );
    }
}
