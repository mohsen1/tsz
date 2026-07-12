//! Exact declaration-node end positions shared by diagnostics and tooling.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{assert_span_on, parse_source};

#[test]
fn bodyless_declarations_with_semicolons_end_before_following_tokens() {
    let source = r#"function plain(): void; const afterPlain = 1;
export default function optional(): void; const afterOptional = 2;
class C {
    method(): void; next() {}
    get value(): string; other() {}
    set value(v: string); final() {}
}"#;
    let (parser, _) = parse_source(source);

    for (kind, expected) in [
        (
            syntax_kind_ext::FUNCTION_DECLARATION,
            "function plain(): void;",
        ),
        (
            syntax_kind_ext::FUNCTION_DECLARATION,
            "function optional(): void;",
        ),
        (syntax_kind_ext::METHOD_DECLARATION, "method(): void;"),
        (syntax_kind_ext::GET_ACCESSOR, "get value(): string;"),
        (syntax_kind_ext::SET_ACCESSOR, "set value(v: string);"),
    ] {
        assert_span_on(&parser, source, kind, expected);
    }
}

#[test]
fn bodyless_declarations_with_asi_end_before_following_trivia() {
    let source = r#"function plain(): void
const afterPlain = 1;
export default function optional(): void
const afterOptional = 2;
class C {
    method(): void
    next() {}
    get value(): string
    other() {}
    set value(v: string)
    final() {}
}"#;
    let (parser, _) = parse_source(source);

    for (kind, expected) in [
        (
            syntax_kind_ext::FUNCTION_DECLARATION,
            "function plain(): void",
        ),
        (
            syntax_kind_ext::FUNCTION_DECLARATION,
            "function optional(): void",
        ),
        (syntax_kind_ext::METHOD_DECLARATION, "method(): void"),
        (syntax_kind_ext::GET_ACCESSOR, "get value(): string"),
        (syntax_kind_ext::SET_ACCESSOR, "set value(v: string)"),
    ] {
        assert_span_on(&parser, source, kind, expected);
    }
}
