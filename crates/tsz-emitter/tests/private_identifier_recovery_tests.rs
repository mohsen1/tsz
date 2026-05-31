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

#[test]
fn bare_hash_private_name_recovery_drives_downlevel_private_emit() {
    let source = r"
#

class C {
    #

    m() {
        this.#
    }
}
";
    let output = print_es2015(source);

    assert!(
        output.contains("#;\nclass C"),
        "top-level recovered bare hash should still print as `#;`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _C_;"),
        "class-body recovered bare hash should allocate the blank private field helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__classPrivateFieldGet(this, _C_, \"f\")"),
        "property access `this.#` should lower through the recovered private helper.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this.\n"),
        "bare hash property access must not print as a dangling `this.`.\nOutput:\n{output}"
    );
}
