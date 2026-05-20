//! Regression coverage for while-loop body variable capture in ES5 emit.
//!
//! When a `while` loop body captures block-scoped variables by closures, the
//! generated `_loop_N` function must take **no parameters** because the
//! captured variables are always body-declared and get fresh scope inside the
//! function. Only `for`-loop initializer and `for-of`/`for-in` iteration
//! variables can be loop-function parameters.

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

/// Body-declared `let` captured by an arrow in a `while` loop must not become
/// a loop-function parameter. The variable gets fresh scope inside `_loop_N`.
#[test]
fn while_body_let_captured_by_arrow_takes_no_params() {
    let output = emit_es5(
        r#"
while (true) {
    let local = null;
    var a = () => local;
}
"#,
    );

    assert!(
        output.contains("var _loop_1 = function ()"),
        "while loop function must take no parameters when all captured vars are body-declared.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function (local)"),
        "body-declared `local` must not become a loop-function parameter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_loop_1();"),
        "call site must pass no arguments.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_loop_1(local)"),
        "call site must not pass `local`.\nOutput:\n{output}"
    );
}

/// Multiple body-declared block-scoped variables all captured — none become params.
#[test]
fn while_body_multiple_lets_captured_take_no_params() {
    let output = emit_es5(
        r#"
declare function process(x: any, y: any): any;
let cond = true;
while (cond) {
    let x = 1;
    let y = 2;
    var result = () => process(x, y);
}
"#,
    );

    assert!(
        output.contains("var _loop_1 = function ()"),
        "while loop function must take no parameters even with multiple captured vars.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function (x") && !output.contains("function (y"),
        "neither `x` nor `y` must become a loop-function parameter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_loop_1();"),
        "call site must pass no arguments.\nOutput:\n{output}"
    );
}

/// Renamed binding variable name — the invariant holds regardless of what letter is chosen.
#[test]
fn while_body_renamed_let_captured_takes_no_params() {
    let output = emit_es5(
        r#"
var value = 0;
while (value < 10) {
    let item = value * 2;
    var capture = () => item;
    value++;
}
"#,
    );

    assert!(
        output.contains("var _loop_1 = function ()"),
        "while loop function must take no parameters (renamed binding variable).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function (item)"),
        "body-declared `item` must not be a parameter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_loop_1();"),
        "call site must pass no arguments.\nOutput:\n{output}"
    );
}

/// `for`-loop initializer variables ARE passed as parameters (existing behavior unaffected).
#[test]
fn for_loop_init_var_is_passed_as_param() {
    let output = emit_es5(
        r#"
for (let i = 0; i < 3; i++) {
    var f = () => i;
}
"#,
    );

    assert!(
        output.contains("function (i)"),
        "`for`-loop initializer variable `i` must be a loop-function parameter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_loop_1(i)"),
        "call site must pass `i`.\nOutput:\n{output}"
    );
}

/// `for-of` iteration variables ARE passed as parameters (existing behavior unaffected).
#[test]
fn for_of_iteration_var_is_passed_as_param() {
    let output = emit_es5(
        r#"
for (const x of [1, 2, 3]) {
    var f = () => x;
}
"#,
    );

    assert!(
        output.contains("function (x)"),
        "`for-of` iteration variable `x` must be a loop-function parameter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_loop_1(x)"),
        "call site must pass `x`.\nOutput:\n{output}"
    );
}
