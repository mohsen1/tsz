//! TS1100 regression tests for `arguments`/`eval` used as a binding name in
//! a destructuring pattern. tsc emits "Invalid use of 'arguments' in strict
//! mode." for these forms; the simple `var arguments` path is already covered
//! by `state/variable_checking/core.rs`, but destructuring patterns flow
//! through `check_binding_element_with_request` and previously skipped the
//! TS1100 check entirely. See conformance test
//! `emitArrowFunctionWhenUsingArguments17_ES6` for the original target.
//!
//! Outside strict mode, none of these forms should fire TS1100.

use tsz_checker::test_utils::{check_source_code_messages as get_diagnostics, check_with_options};
use tsz_common::CheckerOptions;

#[test]
fn arguments_in_object_destructuring_emits_ts1100_in_strict_mode() {
    // The conformance target: `var { arguments } = { arguments: "hello" };`
    // inside a strict-mode function body.
    let source = r#"
"use strict";
function f() {
    var { arguments } = { arguments: "hello" };
}
"#;
    let diags = get_diagnostics(source);
    let ts1100_arguments: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 1100 && msg.contains("arguments"))
        .collect();
    assert!(
        !ts1100_arguments.is_empty(),
        "expected TS1100 for `arguments` in object-destructuring binding (strict mode); got: {diags:#?}"
    );
}

#[test]
fn eval_in_object_destructuring_emits_ts1100_in_strict_mode() {
    let source = r#"
"use strict";
function f() {
    var { eval } = { eval: "hello" };
}
"#;
    let diags = get_diagnostics(source);
    let ts1100_eval: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 1100 && msg.contains("'eval'"))
        .collect();
    assert!(
        !ts1100_eval.is_empty(),
        "expected TS1100 for `eval` in object-destructuring binding (strict mode); got: {diags:#?}"
    );
}

#[test]
fn arguments_in_array_destructuring_emits_ts1100_in_strict_mode() {
    let source = r#"
"use strict";
function f() {
    var [arguments] = ["hello"];
}
"#;
    let diags = get_diagnostics(source);
    let ts1100_arguments: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 1100 && msg.contains("arguments"))
        .collect();
    assert!(
        !ts1100_arguments.is_empty(),
        "expected TS1100 for `arguments` in array-destructuring binding (strict mode); got: {diags:#?}"
    );
}

#[test]
fn arguments_in_destructuring_renaming_emits_ts1100_in_strict_mode() {
    // `{ x: arguments }` renames `x` to local binding `arguments`. The bound
    // name is what matters for TS1100, not the property name.
    let source = r#"
"use strict";
function f() {
    var { x: arguments } = { x: "hello" };
}
"#;
    let diags = get_diagnostics(source);
    let ts1100_arguments: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 1100 && msg.contains("arguments"))
        .collect();
    assert!(
        !ts1100_arguments.is_empty(),
        "expected TS1100 for `{{ x: arguments }}` renaming binding (strict mode); got: {diags:#?}"
    );
}

#[test]
fn arguments_in_destructuring_no_ts1100_outside_strict_mode() {
    // No `"use strict"` directive AND `always_strict` disabled — tsc does not
    // emit TS1100 for `arguments` in a non-strict-mode binding.
    let source = r#"
function f() {
    var { arguments } = { arguments: "hello" };
}
"#;
    let opts = CheckerOptions {
        always_strict: false,
        ..Default::default()
    };
    let diags: Vec<(u32, String)> = check_with_options(source, opts)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect();
    let ts1100_arguments: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 1100 && msg.contains("arguments"))
        .collect();
    assert!(
        ts1100_arguments.is_empty(),
        "TS1100 should NOT fire for `arguments` destructuring outside strict mode; got: {diags:#?}"
    );
}

#[test]
fn nested_destructuring_arguments_emits_ts1100_in_strict_mode() {
    // Nested destructuring: the inner `arguments` should still be caught.
    let source = r#"
"use strict";
function f() {
    var { a: { arguments } } = { a: { arguments: "x" } };
}
"#;
    let diags = get_diagnostics(source);
    let ts1100_arguments: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 1100 && msg.contains("arguments"))
        .collect();
    assert!(
        !ts1100_arguments.is_empty(),
        "expected TS1100 for nested destructuring `arguments` (strict mode); got: {diags:#?}"
    );
}
