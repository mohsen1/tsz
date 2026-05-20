//! Regression coverage for control-flow sentinels in ES5 loop-capture emit.

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
fn continue_in_nested_if_returns_from_loop_function_without_caller_state() {
    let output = emit_es5(
        r#"
function foo() {
    for (const i of [0, 1]) {
        if (i === 0) {
            continue;
        }
        (() => i)();
    }
}
"#,
    );

    assert!(
        output.contains("return \"continue\";"),
        "Nested continue should lower to a loop-function return sentinel.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("continue;"),
        "The original continue must not be emitted inside the loop function.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _state"),
        "A continue-only captured loop does not need caller state dispatch.\nOutput:\n{output}"
    );
}

#[test]
fn break_state_does_not_dispatch_continue_state() {
    let output = emit_es5(
        r#"
function foo() {
    for (const i of [0, 1]) {
        if (i === 0) continue;
        if (i === 1) break;
        (() => i)();
    }
}
"#,
    );

    assert!(
        output.contains("return \"continue\";"),
        "Nested continue should still be represented in the loop function.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return \"break\";"),
        "Nested break should still be represented in the loop function.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (_state === \"break\")"),
        "Break still requires caller state dispatch.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("=== \"continue\""),
        "Continue state is ignored by the caller because the loop call is the whole iteration body.\nOutput:\n{output}"
    );
}
