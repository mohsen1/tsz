//! ES5 parameter-prologue ordering: an object-rest binding parameter moves the
//! whole leading-parameter prologue onto `tsc`'s ES2018 transform path, so the
//! rest-parameter `arguments`-copy loop is emitted *before* the binding/default
//! declarations. Without an object rest the ES2015 transform owns the prologue
//! and emits the binding/default declarations *before* the copy loop.
//!
//! Binder names are varied across cases so the ordering decision is exercised
//! structurally rather than against any fixed identifier (anti-hardcoding gate).

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..PrintOptions::default()
        },
    )
    .code
}

/// Position of `needle` in `haystack`, asserting it appears exactly once.
fn index_of(output: &str, needle: &str) -> usize {
    let first = output
        .find(needle)
        .unwrap_or_else(|| panic!("expected to find {needle:?} in:\n{output}"));
    assert!(
        !output[first + needle.len()..].contains(needle),
        "expected {needle:?} exactly once in:\n{output}"
    );
    first
}

fn assert_before(output: &str, first: &str, second: &str) {
    let a = index_of(output, first);
    let b = index_of(output, second);
    assert!(a < b, "expected {first:?} before {second:?} in:\n{output}");
}

#[test]
fn object_rest_param_emits_arguments_copy_loop_before_preamble() {
    // `tsc`: copy loop first, then `var a = _a.a, rest = __rest(_a, ["a"]);`
    let output = emit_es5(
        "function f({ a, ...rest }: { a: number; b: string }, ...args: number[]) { return rest; }",
    );
    assert_before(&output, "for (var _i = 1;", "rest = __rest(_a, [\"a\"])");
}

#[test]
fn two_object_rest_params_emit_copy_loop_before_both_preambles() {
    let output = emit_es5(
        "function g({ a, ...r1 }: any, { b, ...r2 }: any, ...extra: number[]) { return r1; }",
    );
    assert_before(&output, "for (var _i = 2;", "r1 = __rest(_a, [\"a\"])");
    assert_before(&output, "for (var _i = 2;", "r2 = __rest(_b, [\"b\"])");
    // The two preambles keep source order relative to each other.
    assert_before(
        &output,
        "r1 = __rest(_a, [\"a\"])",
        "r2 = __rest(_b, [\"b\"])",
    );
}

#[test]
fn object_rest_param_moves_default_assignment_after_copy_loop() {
    // The presence of the object rest also pushes a sibling default-valued
    // parameter's `if (... === void 0)` assignment after the copy loop.
    let output =
        emit_es5("function h({ a, ...tail }: any, count = 1, ...items: number[]) { return tail; }");
    assert_before(&output, "for (var _i = 2;", "tail = __rest(_a, [\"a\"])");
    assert_before(
        &output,
        "tail = __rest(_a, [\"a\"])",
        "if (count === void 0)",
    );
}

#[test]
fn object_rest_param_after_plain_and_array_params_orders_copy_loop_first() {
    let output = emit_es5(
        "function k(p: number, { a, ...rest }: any, [b, c]: number[], ...args: number[]) { return rest; }",
    );
    assert_before(&output, "for (var _i = 3;", "rest = __rest(_a, [\"a\"])");
    assert_before(&output, "rest = __rest(_a, [\"a\"])", "c = _b[1]");
}

#[test]
fn nested_object_rest_in_array_param_orders_copy_loop_first() {
    // Object rest nested inside an array binding pattern still triggers the flip.
    let output =
        emit_es5("function n([{ a, ...rest }]: any[], ...args: number[]) { return rest; }");
    assert_before(&output, "for (var _i = 1;", "rest = __rest");
}

// --- Negative controls: no object rest -> binding/default prologue stays first ---

#[test]
fn array_binding_param_keeps_preamble_before_copy_loop() {
    let output = emit_es5("function f([a, b]: number[], ...args: number[]) { return a; }");
    assert_before(&output, "a = _a[0], b = _a[1]", "for (var _i = 1;");
}

#[test]
fn default_param_keeps_assignment_before_copy_loop() {
    let output = emit_es5("function f(x = 1, ...args: number[]) { return x; }");
    assert_before(&output, "if (x === void 0)", "for (var _i = 1;");
}

#[test]
fn object_binding_without_rest_keeps_preamble_before_copy_loop() {
    let output = emit_es5("function f({ a }: any, value = 1, ...args: number[]) { return a; }");
    assert_before(&output, "a = _a.a", "if (value === void 0)");
    assert_before(&output, "if (value === void 0)", "for (var _i = 2;");
}

#[test]
fn object_rest_only_in_rest_param_keeps_leading_param_order() {
    // The object rest lives on the rest parameter's own binding, so the leading
    // `[a, b]` param prologue stays *before* the copy loop (no flip).
    let output = emit_es5("function f([a, b]: number[], ...{ c, ...r }: any) { return r; }");
    assert_before(&output, "a = _a[0], b = _a[1]", "for (var _i = 1;");
}
