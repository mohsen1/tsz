//! Regression tests for #16259, the sibling family to #16251/#16262: several call sites
//! captured a node's `end` via `token_end()` *after* calling `parse_expected(CloseBraceToken)`.
//! `parse_expected` advances the scanner past the matched token on success, so `token_end()`
//! afterward returns the end of the *next* token, not of the just-consumed `}`. Each site here
//! is fixed the same way #16251 fixed `ComputedPropertyName`: capture `token_end()` while the
//! scanner is still positioned on `}`, before `parse_expected` consumes it.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{assert_span, assert_span_on, parse_source, parse_source_named};

#[test]
fn jsx_spread_attribute_ends_before_the_following_attribute() {
    // Before the fix, the span extended through the following ` b` text. JSX requires a
    // `.tsx` file name to parse at all.
    let source = "const x = <div {...a} b />;";
    let (parser, _) = parse_source_named("test.tsx", source);
    assert_span_on(
        &parser,
        source,
        syntax_kind_ext::JSX_SPREAD_ATTRIBUTE,
        "{...a}",
    );
}

#[test]
fn jsx_expression_attribute_initializer_ends_before_the_following_attribute() {
    let source = "const x = <div a={1} b />;";
    let (parser, _) = parse_source_named("test.tsx", source);
    assert_span_on(&parser, source, syntax_kind_ext::JSX_EXPRESSION, "{1}");
}

#[test]
fn class_expression_ends_before_the_trailing_semicolon_and_call() {
    // Before the fix, the span extended through `()` in the IIFE call.
    let source = "const C = (class { m() {} })();";
    assert_span(
        source,
        syntax_kind_ext::CLASS_EXPRESSION,
        "class { m() {} }",
    );
}

#[test]
fn class_declaration_ends_before_the_following_statement() {
    let source = "class Foo { m() {} } const x = 1;";
    assert_span(
        source,
        syntax_kind_ext::CLASS_DECLARATION,
        "class Foo { m() {} }",
    );
}

#[test]
fn abstract_class_declaration_ends_before_the_following_statement() {
    let source = "abstract class Foo { m(): void; } const x = 1;";
    assert_span(
        source,
        syntax_kind_ext::CLASS_DECLARATION,
        "abstract class Foo { m(): void; }",
    );
}

#[test]
fn decorated_abstract_class_declaration_ends_before_the_following_statement() {
    let source = "@dec abstract class Foo { m(): void; } const x = 1;";
    assert_span(
        source,
        syntax_kind_ext::CLASS_DECLARATION,
        "@dec abstract class Foo { m(): void; }",
    );
}

#[test]
fn class_static_block_ends_before_the_following_member() {
    let source = "class Foo { static { a; } m() {} }";
    assert_span(
        source,
        syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION,
        "static { a; }",
    );
}

#[test]
fn import_attributes_end_before_the_trailing_semicolon() {
    // The `ImportAttributes` node's own span starts at `with`, not `{`.
    let source = r#"import x from "x" with { type: "json" };"#;
    assert_span(
        source,
        syntax_kind_ext::IMPORT_ATTRIBUTES,
        r#"with { type: "json" }"#,
    );
}

#[test]
fn switch_statement_ends_before_the_following_statement() {
    // Before the fix, the switch statement's own `end` overshot into the following
    // statement even though the nested `CaseBlock`'s own `end` (a sibling capture in
    // the same function) was already correct.
    let source = "switch (a) { case 1: break; } const x = 1;";
    assert_span(
        source,
        syntax_kind_ext::SWITCH_STATEMENT,
        "switch (a) { case 1: break; }",
    );
}

#[test]
fn two_class_static_blocks_in_the_same_class_each_end_correctly() {
    // Guards against a fix that only works for the first occurrence.
    let source = "class Foo { static { a; } static { b; } }";
    let (parser, _) = parse_source(source);
    for expected in ["static { a; }", "static { b; }"] {
        assert_span_on(
            &parser,
            source,
            syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION,
            expected,
        );
    }
}

#[test]
fn missing_close_brace_recovery_does_not_regress() {
    // Error-recovery path (no `}`): `parse_expected` does not advance the scanner when
    // the expected token is absent, so this shape was never affected by the bug, but it
    // is worth pinning so a future change to the fix doesn't silently regress it.
    let source = "class Foo { m() {}";
    let (parser, _) = parse_source(source);
    assert!(
        !parser.get_diagnostics().is_empty(),
        "missing '}}' should still be reported"
    );
}
