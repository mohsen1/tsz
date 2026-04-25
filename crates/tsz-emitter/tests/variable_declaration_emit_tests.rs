#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print;

#[test]
fn empty_let_declaration_has_no_space_before_semicolon() {
    let source = "\"use strict\";\nlet;";
    let output = parse_and_print(source);

    assert!(output.contains("\nlet;"), "unexpected output: {output}");
    assert!(!output.contains("\nlet ;"), "unexpected output: {output}");
}
