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
