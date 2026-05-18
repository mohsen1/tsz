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

#[test]
fn recovered_jsdoc_nullable_type_arguments_are_preserved() {
    let source = "declare function foo<T>(x: T): T;\n\
const a = foo<?>;\n\
const b = foo<string?>;\n\
const c = foo<?string>;\n\
const d = foo<?string?>;\n";
    let output = print_es2015(source);

    assert!(
        output.contains("const a = foo<?>;"),
        "Bare JSDoc wildcard type argument must stay attached to the expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const b = foo<?string>;"),
        "Postfix JSDoc nullable type argument should be recovered in tsc's prefix form.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const c = foo<?string>;"),
        "Prefix JSDoc nullable type argument should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const d = foo<??string>;"),
        "Combined prefix/postfix JSDoc nullable type argument should match tsc recovery.\nOutput:\n{output}"
    );
}

#[test]
fn recovered_jsdoc_nullable_type_arguments_work_for_class_values() {
    let source = "declare class Box<T> {}\n\
const a = Box<string?>;\n\
const b = Box<?number?>;\n";
    let output = print_es2015(source);

    assert!(
        output.contains("const a = Box<?string>;"),
        "Class value instantiation recovery should preserve postfix nullable type arguments.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const b = Box<??number>;"),
        "Class value instantiation recovery should preserve combined nullable markers.\nOutput:\n{output}"
    );
}
