//! Integration tests for malformed class member emit recovery.

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

#[test]
fn public_empty_block_member_emits_recovered_block_statement() {
    let output = print_es2015("class C {\n    public {};\n}\n");
    assert_eq!(output, "class C {\n}\n{ }\n;\n");
}

#[test]
fn public_index_signature_block_member_emits_recovered_block_statement() {
    let output = print_es2015("class C {\n    public {[name:string]:VariableDeclaration};\n}\n");
    assert_eq!(
        output,
        "class C {\n}\n{\n    [name, string];\n    VariableDeclaration;\n}\n;\n"
    );
}
