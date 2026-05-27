//! ES5 downlevel-iteration `for-of` and rest-parameter lowering inside
//! generator/async bodies (issue #8510).
//!
//! Rule: when a non-suspending `for-of` appears inside an async/generator body
//! being downleveled to the `__generator` state machine and
//! `downlevelIteration` is enabled, `tsc` lowers it with the `__values`
//! iterator protocol (try/catch/finally), exactly as it does anywhere else —
//! not the array-index fast path. A trailing rest parameter on a downleveled
//! generator is also lowered to the `arguments`-copy prologue.

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es5_downlevel(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        remove_comments: true,
        downlevel_iteration: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn generator_rest_param_and_for_of_match_tsc() {
    // The reported repro (restParameterInDownlevelGenerator).
    let output = emit_es5_downlevel(
        "function * mergeStringLists(...strings: string[]) {
            for (var str of strings);
        }",
    );

    // Rest parameter is downleveled to the arguments-copy prologue.
    assert!(
        output.contains("function mergeStringLists() {"),
        "rest param should be dropped from the signature.\n{output}"
    );
    assert!(
        output.contains("var strings = [];"),
        "rest target should be declared.\n{output}"
    );
    assert!(
        output.contains("for (_i = 0; _i < arguments.length; _i++) {")
            && output.contains("strings[_i] = arguments[_i];"),
        "rest copy loop with hoisted _i index.\n{output}"
    );
    // for-of uses the __values iterator protocol, not the array-index form.
    assert!(
        output.contains("__values(strings)") && output.contains("strings_1.next()"),
        "for-of should use the __values iterator protocol.\n{output}"
    );
    assert!(
        !output.contains("strings_1.length"),
        "for-of must not use the array-index fast path under downlevelIteration.\n{output}"
    );
    // The __generator state name skips _a (consumed by the iterator return temp).
    assert!(
        output.contains("__generator(this, function (_b)"),
        "generator state name should advance past the for-of temps.\n{output}"
    );
}

#[test]
fn generator_for_of_downlevel_is_not_name_keyed() {
    // Same rule with different identifier spellings — proves the fix is
    // structural, not keyed to `strings`/`str`.
    let output = emit_es5_downlevel(
        "function * f(...items: number[]) {
            for (var x of items);
        }",
    );
    assert!(output.contains("function f() {"), "{output}");
    assert!(output.contains("var items = [];"), "{output}");
    assert!(
        output.contains("__values(items)") && output.contains("items_1.next()"),
        "{output}"
    );
    assert!(!output.contains("items_1.length"), "{output}");
}

#[test]
fn generator_for_of_downlevel_without_rest_param() {
    // for-of under downlevelIteration with no rest parameter: __values form,
    // no arguments-copy prologue.
    let output = emit_es5_downlevel(
        "function * g(xs: number[]) {
            for (var x of xs);
        }",
    );
    assert!(
        output.contains("function g(xs) {"),
        "non-rest param stays in the signature.\n{output}"
    );
    assert!(
        output.contains("__values(xs)"),
        "for-of should use __values.\n{output}"
    );
    assert!(
        !output.contains("var xs = [];"),
        "no rest prologue when there is no rest parameter.\n{output}"
    );
}

#[test]
fn async_function_for_of_downlevel_uses_values() {
    // The broad rule covers async bodies too: a non-suspending for-of under
    // downlevelIteration lowers via __values inside the __awaiter/__generator.
    let output = emit_es5_downlevel(
        "async function a(xs: number[]) {
            for (var x of xs);
            await xs;
        }",
    );
    assert!(
        output.contains("__values(xs)"),
        "async non-suspending for-of should use __values under downlevelIteration.\n{output}"
    );
    assert!(
        !output.contains("xs_1.length"),
        "async for-of must not use the array-index fast path under downlevelIteration.\n{output}"
    );
}

#[test]
fn generator_rest_param_without_for_of() {
    // Rest parameter lowering on a generator without any for-of.
    let output = emit_es5_downlevel(
        "function * g(...args: number[]) {
            yield args.length;
        }",
    );
    assert!(output.contains("function g() {"), "{output}");
    assert!(output.contains("var args = [];"), "{output}");
    assert!(
        output.contains("args[_i] = arguments[_i];"),
        "rest copy loop body.\n{output}"
    );
}
