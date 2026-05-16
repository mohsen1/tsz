#[path = "test_support.rs"]
mod test_support;

use test_support::{parse_and_lower_print, parse_and_print};
use tsz_emitter::output::printer::PrintOptions;

#[test]
fn empty_let_declaration_has_no_space_before_semicolon() {
    let source = "\"use strict\";\nlet;";
    let output = parse_and_print(source);

    assert!(output.contains("\nlet;"), "unexpected output: {output}");
    assert!(!output.contains("\nlet ;"), "unexpected output: {output}");
}

#[test]
fn object_rest_without_initializer_recovery_stays_syntactically_valid() {
    let source = "const { ...rest };";
    let output = parse_and_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("const _a = void 0, rest = __rest(_a, []);"),
        "unexpected output: {output}"
    );
    assert!(
        !output.contains("= ,"),
        "object rest recovery should not emit an empty assignment RHS: {output}"
    );
}

#[test]
fn rest_parameter_object_binding_with_rest_emits_body_preamble() {
    let source =
        "function e(...{0: a = 1, 1: b = true, ...rest: rest}: [boolean, string, number]) { }";
    let output = parse_and_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains(
            "function e(..._a) { var { 0: a = 1, 1: b = true } = _a, rest = __rest(_a, [\"0\", \"1\"]); }"
        ),
        "rest parameter object binding should preserve rest syntax and emit the object-rest preamble.\nOutput:\n{output}"
    );
}

#[test]
fn recovered_empty_variable_initializer_preserves_equals() {
    let source = "var NUMBER1 = var NUMBER-;";
    let output = parse_and_print(source);

    assert!(
        output.contains("var NUMBER1 = ;"),
        "unexpected output: {output}"
    );
}

#[test]
fn malformed_void_qualified_type_recovers_following_declaration() {
    let source = "\"use strict\";\nvar v : void.x;";
    let output = parse_and_print(source);

    assert!(
        output.contains("\"use strict\";\nvar v, x;"),
        "unexpected output: {output}"
    );
}

#[test]
fn invalid_unicode_escape_declaration_tail_recovers_as_same_var_list() {
    let output = parse_and_print(r"var arg\u003");

    assert!(
        output.contains("var arg, u003;"),
        "malformed unicode escape after a declarator should recover as a following declarator.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var arg;\nu003;"),
        "malformed unicode escape tail must not fall out as a separate expression statement.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_unicode_escape_declaration_tail_keeps_non_hex_debris() {
    let output = parse_and_print(r"var arg\uxxxx");

    assert!(
        output.contains("var arg, uxxxx;"),
        "non-hex unicode escape debris after a declarator should recover as a following declarator.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_unicode_escape_declaration_name_merges_adjacent_identifier_part() {
    let output = parse_and_print(r"var \u0031a; // 1a is an invalid identifier");

    assert!(
        output.contains("var u0031a; // 1a is an invalid identifier"),
        "invalid unicode escape at declaration-name start should drop only the leading backslash and keep adjacent identifier text.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var a;"),
        "invalid declaration-name recovery must not emit only the adjacent identifier token.\nOutput:\n{output}"
    );
}

#[test]
fn valid_unicode_escape_inside_identifier_is_unchanged() {
    let output = parse_and_print(r"var a\u0031; // a1 is a valid identifier");

    assert!(
        output.contains(r"var a\u0031; // a1 is a valid identifier"),
        "valid identifier unicode escapes should keep the normal scanner/parser spelling.\nOutput:\n{output}"
    );
}

#[test]
fn string_literal_with_crlf_line_continuation_does_not_double_semicolon() {
    // Regression for `sourceMap-StringLiteralWithNewLine` and the `literals`
    // / `sourceMap-LineBreaks` baselines: when a string literal uses an
    // ECMAScript LineContinuation (`\` followed by `\r\n`) inside a CRLF
    // source file, the raw-string read in `get_raw_string_literal` was
    // consuming only one byte after `\\` (the `\r`), then tripping the
    // line-terminator break on the trailing `\n`. The recovery branch then
    // appended an extra `;` after the surviving quote, producing `";;`.
    let source = "namespace Foo {\r\n    var y = \"test\\\r\nfun\";\r\n}\r\n";
    let output = parse_and_print(source);

    assert!(
        !output.contains("\";;"),
        "CRLF line continuation must not append an extra semicolon.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"test\\\nfun\";") || output.contains("\"test\\\r\nfun\";"),
        "Continuation string body should be preserved.\nOutput:\n{output}"
    );
}
