//! Regression coverage for labeled `var` declarations inside ES5 loop-capture IIFEs.

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;
use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::PrintOptions;

fn emit_es5(source: &str) -> String {
    parse_and_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    )
}

#[test]
fn initializerless_labeled_var_becomes_empty_labeled_statement_in_loop_iife() {
    let output = emit_es5(
        r#"
for (let x of []) {
    var v0 = x;
    foo: var;
    (function () { return x + v0; });
}
"#,
    );

    assert!(
        output.contains("    foo: ;"),
        "Recovered labeled `var;` should become an empty labeled statement inside the loop IIFE.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var v0;"),
        "The var binding should still be hoisted outside the captured loop.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("foo: var"),
        "Loop-capture emission must not leave a `var` keyword under the label.\nOutput:\n{output}"
    );
}

#[test]
fn initializerless_labeled_var_with_name_is_hoisted_and_erased_in_loop_iife() {
    let output = emit_es5(
        r#"
for (let x of []) {
    var v0 = x;
    foo: var y;
    (function () { return x + v0; });
}
"#,
    );

    assert!(
        output.contains("    foo: ;"),
        "Recovered labeled `var y;` should become an empty labeled statement inside the loop IIFE.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var v0, y;"),
        "The labeled var binding should participate in the captured-loop hoist.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("foo: var y"),
        "Loop-capture emission must not leave the recovered declaration under the label.\nOutput:\n{output}"
    );
}
