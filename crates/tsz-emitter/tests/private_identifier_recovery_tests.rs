//! Regression coverage for private identifiers in parser-recovery emit.

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

#[test]
fn private_identifier_in_array_assignment_recovery_is_preserved() {
    let output = print_es2015("[#abc]=\n");

    assert!(
        output.contains("[#abc] ="),
        "array assignment recovery must preserve the private identifier; output:\n{output}"
    );
    assert!(
        !output.contains("[] ="),
        "array assignment recovery must not erase the private identifier; output:\n{output}"
    );
}
