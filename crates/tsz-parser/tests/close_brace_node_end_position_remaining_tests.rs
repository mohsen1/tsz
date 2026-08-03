//! Regression tests for the 4 "not yet verified" candidate sites #16265 named as siblings
//! of #16251/#16259/#16262: a node's `end` captured via `token_end()` *after* calling
//! `parse_expected(CloseBraceToken)` reads the end of the *next* token instead of the
//! just-consumed `}`, because `parse_expected` advances the scanner past the matched token
//! on success. Each site here is fixed the same way: capture `token_end()` while the scanner
//! is still positioned on `}`, before `parse_expected` consumes it.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::assert_span;

#[test]
fn named_exports_end_before_the_following_from_clause() {
    // `NamedExports`'s own span is just the brace list; the `from "m"` clause belongs to
    // the outer `ExportDeclaration`. Before the fix, the span extended through `from`.
    let source = r#"export { a, b } from "m";"#;
    assert_span(source, syntax_kind_ext::NAMED_EXPORTS, "{ a, b }");
}

#[test]
fn named_imports_end_before_the_following_statement() {
    let source = r#"import { a, b } from "m"; const x = 1;"#;
    assert_span(source, syntax_kind_ext::NAMED_IMPORTS, "{ a, b }");
}

#[test]
fn named_imports_with_asterisk_recovery_ends_at_the_consumed_close_brace() {
    // `{ * }` is malformed (asterisk isn't a valid specifier); the parser's recovery path
    // consumes the closing `}` itself mid-loop rather than falling through to the shared
    // end-of-function `parse_expected(CloseBraceToken)` call. This exercises that separate
    // capture point, not just the common-case one the other tests above cover.
    let source = r#"import { * } from "m"; const x = 1;"#;
    assert_span(source, syntax_kind_ext::NAMED_IMPORTS, "{ * }");
}

#[test]
fn module_block_ends_before_the_following_statement() {
    let source = "namespace N { const x = 1; } const y = 2;";
    assert_span(source, syntax_kind_ext::MODULE_BLOCK, "{ const x = 1; }");
}

#[test]
fn interface_declaration_ends_before_the_following_statement() {
    let source = "interface Foo { m(): void; } const x = 1;";
    assert_span(
        source,
        syntax_kind_ext::INTERFACE_DECLARATION,
        "interface Foo { m(): void; }",
    );
}

#[test]
fn type_literal_ends_before_the_following_union_member() {
    // Before the fix, the span extended through ` | string`.
    let source = "type T = { a: number } | string;";
    assert_span(source, syntax_kind_ext::TYPE_LITERAL, "{ a: number }");
}

#[test]
fn mapped_type_ends_before_the_following_union_member() {
    let source = "type T = { [K in Keys]: number } | string;";
    assert_span(
        source,
        syntax_kind_ext::MAPPED_TYPE,
        "{ [K in Keys]: number }",
    );
}
