//! Integration tests for comma-expression emit recovery.

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

#[test]
fn recovered_comma_expressions_preserve_missing_operands() {
    let source = "(ANY, );\n(, ANY);\n( , );\n";
    let output = print_es2015(source);

    assert!(
        output.contains("(ANY, );"),
        "missing comma RHS should preserve the comma expression; output:\n{output}"
    );
    assert!(
        output.contains("(, ANY);"),
        "missing comma LHS should preserve the comma expression; output:\n{output}"
    );
    assert!(
        output.contains("(, );"),
        "missing comma operands should preserve both comma positions; output:\n{output}"
    );
}
