//! Integration tests for erasing expression type arguments in JS emit.

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

#[test]
fn import_type_arguments_statement_erases_without_parens() {
    let source = "import<T>\nconst a = import<string, number>\n";
    let output = print_es2015(source);

    assert!(
        output.contains("import;"),
        "statement-position import<T> should erase to bare import; output:\n{output}"
    );
    assert!(
        output.lines().next() == Some("import;"),
        "statement-position import<T> should not be parenthesized; output:\n{output}"
    );
    assert!(
        output.contains("const a = (import);"),
        "value-position import<T> should retain tsc-style parens; output:\n{output}"
    );
}
