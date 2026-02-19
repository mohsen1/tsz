//! Tests for parser handling of `let` as identifier vs keyword.
//! When `let` is not followed by a valid declaration start (identifier, `{`, `[`),
//! it should be parsed as an identifier expression (not a variable declaration),
//! matching tsc behavior. The checker will then emit TS1214 in strict mode.

use tsz_parser::parser::ParserState;

fn get_parser_error_codes(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let mut codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    codes.sort();
    codes
}

#[test]
fn let_semicolon_not_parsed_as_empty_var_decl() {
    // `let;` should NOT produce TS1123 (variable declaration list cannot be empty)
    // because `let` should be treated as an identifier, not a declaration keyword
    let codes = get_parser_error_codes("export var a;\nlet;\n");
    assert!(
        !codes.contains(&1123),
        "Should NOT emit TS1123 for `let;`, got parser errors: {codes:?}"
    );
}

#[test]
fn var_semicolon_still_emits_ts1123() {
    // `var;` should still emit TS1123
    let codes = get_parser_error_codes("var;\n");
    assert!(
        codes.contains(&1123),
        "Expected TS1123 for `var;`, got: {codes:?}"
    );
}

#[test]
fn const_semicolon_still_emits_ts1123() {
    // `const;` should still emit TS1123
    let codes = get_parser_error_codes("const;\n");
    assert!(
        codes.contains(&1123),
        "Expected TS1123 for `const;`, got: {codes:?}"
    );
}

#[test]
fn let_with_identifier_parsed_as_declaration() {
    // `let x = 1;` should parse normally as a let declaration, no parser errors
    let codes = get_parser_error_codes("let x = 1;\n");
    assert!(
        codes.is_empty(),
        "Expected no parser errors for `let x = 1;`, got: {codes:?}"
    );
}

#[test]
fn let_with_destructuring_parsed_as_declaration() {
    // `let { a } = { a: 1 };` should parse normally as a let declaration
    let codes = get_parser_error_codes("let { a } = { a: 1 };\n");
    assert!(
        codes.is_empty(),
        "Expected no parser errors for destructuring let, got: {codes:?}"
    );
}

#[test]
fn let_with_array_destructuring_parsed_as_declaration() {
    // `let [a] = [1];` should parse normally as a let declaration
    let codes = get_parser_error_codes("let [a] = [1];\n");
    assert!(
        codes.is_empty(),
        "Expected no parser errors for array destructuring let, got: {codes:?}"
    );
}
