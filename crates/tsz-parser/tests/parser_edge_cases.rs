//! Parser edge cases for scanner/lexer boundary behavior and recovery.
//! These tests focus on hotspots that commonly regress in parser state-machine transitions.

use tsz_parser::parser::ParserState;

fn parse_with_file_name(source: &str, file_name: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let _root = parser.parse_source_file();
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect()
}

fn parse_ok(source: &str, file_name: &str) {
    let diags = parse_with_file_name(source, file_name);
    assert!(
        diags.is_empty(),
        "expected no diagnostics for `{source}`, got: {diags:?}"
    );
}

// =========================================================================
// Regex/Template boundary cases
// =========================================================================

#[test]
fn parser_no_errors_for_literal_regex_in_initializer() {
    parse_ok("const re = /foo/i;", "test.ts");
}

#[test]
fn parser_no_errors_for_nested_regex_in_conditional() {
    parse_ok("const ok = /foo/.test(\"bar\") || /baz/.test(\"qux\");", "test.ts");
}

#[test]
fn parser_no_errors_for_regex_and_division_context_switches() {
    parse_ok("const n = 10 / 2; const re = /foo/gim; const next = 3 / 4;", "test.ts");
}

#[test]
fn parser_no_errors_for_binary_division_expression() {
    parse_ok("const n = 10 / 2 / 5;", "test.ts");
}

#[test]
fn parser_no_errors_for_template_head_middle_tail() {
    parse_ok("const msg = `a${1}b${2}c`;", "test.ts");
}

#[test]
fn parser_no_errors_for_template_nested_expressions() {
    parse_ok("const msg = `${a + 1}${b ? 'x' : 'y'}`;", "test.ts");
}

#[test]
fn parser_no_errors_for_generic_arrow_parameter_list_in_tsx() {
    parse_ok("const id = <T>(x: T) => x;", "test.tsx");
}

#[test]
fn parser_no_errors_for_angle_bracket_type_assertion() {
    parse_ok("let n = <number><unknown>42;", "test.ts");
}

#[test]
fn parser_no_errors_for_tagged_template_start() {
    parse_ok("const tag = (x: string) => x; tag`value ${1}`;", "test.ts");
}

#[test]
fn parser_emits_error_for_unterminated_template_expression() {
    let diags = parse_with_file_name("const msg = `a${1 + 2`;", "test.ts");
    assert!(
        !diags.is_empty(),
        "expected syntax diagnostics for unterminated template, got: {diags:?}"
    );
}

#[test]
fn parser_emits_error_for_unclosed_regex_literal() {
    let diags = parse_with_file_name("const re = /foo[abc", "test.ts");
    assert!(
        !diags.is_empty(),
        "expected parse diagnostics for unterminated regex, got: {diags:?}"
    );
}

#[test]
fn parser_no_errors_for_jsx_attribute_expression() {
    parse_ok("const el = <A foo={1 + 2} bar={false}>{x}</A>;", "test.tsx");
}

// =========================================================================
// JSX boundary cases (.tsx)
// =========================================================================

#[test]
fn parser_no_errors_for_simple_jsx_element() {
    parse_ok("const el = <div id=\"app\">hello</div>;", "test.tsx");
}

#[test]
fn parser_no_errors_for_self_closing_jsx_with_expression() {
    parse_ok("const el = <span value={1 + 2} />;", "test.tsx");
}

#[test]
fn parser_no_errors_for_jsx_fragment() {
    parse_ok("const el = <>left {1} right</>;", "test.tsx");
}

#[test]
fn parser_no_errors_for_jsx_spread_attribute() {
    parse_ok("const el = <Comp {...{a: 1}} b=\"x\" />;", "test.tsx");
}

#[test]
fn parser_no_errors_for_nested_jsx() {
    parse_ok(
        "const el = <A><B c={2}><C /></B></A>;",
        "test.tsx",
    );
}

#[test]
fn parser_no_errors_for_jsx_spread_props_on_fragment() {
    parse_ok("const el = <><A />{1}</>;", "test.tsx");
}

#[test]
fn parser_no_errors_for_jsx_with_template_expression() {
    parse_ok("const el = <A title={`item ${123}`}>value</A>;", "test.tsx");
}

#[test]
fn parser_emits_error_for_malformed_jsx_closing_tag() {
    let diags = parse_with_file_name("const el = <A><B></A>;", "test.tsx");
    assert!(
        !diags.is_empty(),
        "expected JSX diagnostics for malformed closing tag, got: {diags:?}"
    );
}

// =========================================================================
// Parser recovery around punctuation/greater-than rescans
// =========================================================================

#[test]
fn parser_no_errors_with_greater_token_rescan() {
    parse_ok("const n = 1 >> 2 >>> 3;", "test.ts");
}

#[test]
fn parser_no_errors_with_shift_assignment_variants() {
    parse_ok("let n = 1; n >>= 2; n >>>= 1;", "test.ts");
}

#[test]
fn parser_no_errors_with_generic_arrow_shift_expression() {
    parse_ok("const f = <T>(x: T) => x >> 0;", "test.ts");
}

#[test]
fn parser_no_errors_with_generic_comparison_chain() {
    parse_ok("const t = a > b ? a : b;", "test.ts");
}

#[test]
fn parser_emits_error_for_incomplete_shift_expression() {
    let diags = parse_with_file_name("const n = 1 >> ;", "test.ts");
    assert!(
        !diags.is_empty(),
        "expected syntax error for incomplete shift expression, got: {diags:?}"
    );
}

#[test]
fn parser_emits_error_for_unclosed_template_tail() {
    let diags = parse_with_file_name("const label = `left ${1 + 2`;", "test.ts");
    assert!(
        !diags.is_empty(),
        "expected template recovery diagnostics, got: {diags:?}"
    );
}

// =========================================================================
// Misc edge cases from scanner/diagnostic handoff
// =========================================================================

#[test]
fn parser_no_errors_for_bare_unicode_escape_identifiers() {
    parse_ok("const x\\u0061bc = 1;", "test.ts");
}

#[test]
fn parser_no_errors_for_private_identifiers() {
    parse_ok("class C { #x = 1; getX() { return this.#x; } }", "test.ts");
}

#[test]
fn parser_no_errors_for_bigint_and_binary_literals() {
    parse_ok("const n = 0b1010 + 0o77 + 1_000 + 1n;", "test.ts");
}
