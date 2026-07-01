//! ES5 `__awaiter` callback multi-line shape follows the source-line rule.
//!
//! When an `async` method/arrow is lowered to
//! `__awaiter(..., function () { ... return __generator(...) })` at target ES5,
//! `tsc` keeps the generator callback body **inline** for a single-line source
//! body — even when the body hoists `var` groups — and only breaks it across
//! lines for a multi-line source body. The plain async function/expression path
//! already followed this rule; the object-method, class-field async-arrow, and
//! async auto-accessor-arrow paths instead keyed the shape on hoisted-`var`
//! presence (`!hoisted_var_groups.is_empty()`), which diverged from `tsc` in
//! both directions:
//!   * single-line body **with** a hoisted `var` was wrongly broken multi-line;
//!   * multi-line body **without** a hoisted `var` was wrongly kept inline.
//!
//! These tests pin the corrected source-line rule across all three sites, in
//! both directions, with varied binder names (the decision is structural over
//! the source body span, not over any identifier). Output verified
//! byte-compatible with `tsc` target ES5 for each shape.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::CommonJS,
        remove_comments: true,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

// --- Object async methods ------------------------------------------------

/// Single-line object async method that hoists a `var` keeps the `__awaiter`
/// callback inline (was wrongly broken multi-line pre-fix).
#[test]
fn object_method_single_line_with_hoisted_var_stays_inline() {
    let output = emit_es5("const bag_a = { async run_a() { var tmp_a = 1; return tmp_a; } };");

    assert!(
        output.contains("function () { var tmp_a; return __generator(this, function (_a) {"),
        "single-line object async method must keep the hoisted-var callback inline.\nOutput:\n{output}"
    );
}

/// Multi-line object async method that hoists a `var` breaks the callback
/// across lines (unchanged: already correct, guards against over-inlining).
#[test]
fn object_method_multi_line_with_hoisted_var_breaks() {
    let output = emit_es5(
        "const bag_b = {\n  async run_b() {\n    var tmp_b = 1;\n    return tmp_b;\n  }\n};",
    );

    assert!(
        !output.contains("function () { var tmp_b;"),
        "multi-line object async method must break the callback across lines.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function () {\n            var tmp_b;"),
        "multi-line object async method must declare the hoisted var on its own line.\nOutput:\n{output}"
    );
}

// --- Class async methods (instance / static) -----------------------------

/// Single-line class async method that hoists a `var` keeps the callback
/// inline.
#[test]
fn class_method_single_line_with_hoisted_var_stays_inline() {
    let output = emit_es5("class Cls_h { async run_h() { var tmp_h = 1; return tmp_h; } }");

    assert!(
        output.contains("function () { var tmp_h; return __generator(this, function (_a) {"),
        "single-line class async method must keep the hoisted-var callback inline.\nOutput:\n{output}"
    );
}

/// Multi-line class async method breaks the callback across lines. Pre-fix the
/// hardcoded `multiline_callback: false` wrongly kept this inline.
#[test]
fn class_method_multi_line_breaks() {
    let output =
        emit_es5("class Cls_i {\n  async run_i() {\n    var tmp_i = 1;\n    return tmp_i;\n  }\n}");

    assert!(
        !output.contains("function () { var tmp_i;"),
        "multi-line class async method must break the callback across lines.\nOutput:\n{output}"
    );
    assert!(
        output.contains("void 0, function () {\n"),
        "multi-line class async method callback body must start on a new line.\nOutput:\n{output}"
    );
}

/// Multi-line `static` async method breaks the callback across lines too (the
/// static member-emission path shares the same hardcoded-`false` bug pre-fix).
#[test]
fn static_class_method_multi_line_breaks() {
    let output = emit_es5(
        "class Cls_j {\n  static async run_j() {\n    var tmp_j = 1;\n    return tmp_j;\n  }\n}",
    );

    assert!(
        !output.contains("function () { var tmp_j;"),
        "multi-line static async method must break the callback across lines.\nOutput:\n{output}"
    );
}

// --- Class-field async arrows --------------------------------------------

/// Single-line class-field async arrow that hoists a `var` keeps the callback
/// inline (was wrongly broken multi-line pre-fix).
#[test]
fn class_field_arrow_single_line_with_hoisted_var_stays_inline() {
    let output =
        emit_es5("class Cls_c { field_c = async () => { var tmp_c = 2; return tmp_c; }; }");

    assert!(
        output.contains("function () { var tmp_c; return __generator(this, function (_a) {"),
        "single-line class-field async arrow must keep the hoisted-var callback inline.\nOutput:\n{output}"
    );
}

/// Multi-line class-field async arrow with **no** hoisted `var` breaks the
/// callback across lines. Pre-fix the hoisted-var predicate wrongly kept this
/// inline (the reverse-direction witness).
#[test]
fn class_field_arrow_multi_line_without_hoisted_var_breaks() {
    let output = emit_es5("class Cls_d { field_d = async () => {\n    return 1;\n  }; }");

    assert!(
        !output.contains("function () { return __generator"),
        "multi-line class-field async arrow must break the callback across lines even with no hoisted var.\nOutput:\n{output}"
    );
    assert!(
        output.contains("void 0, function () {\n"),
        "multi-line class-field async arrow callback body must start on a new line.\nOutput:\n{output}"
    );
}

/// A single-line class-field async arrow with **no** hoisted `var` stays inline
/// (control: the source-line rule and the old predicate agree here).
#[test]
fn class_field_arrow_single_line_without_hoisted_var_stays_inline() {
    let output = emit_es5("class Cls_e { field_e = async () => { return 3; }; }");

    assert!(
        output.contains("function () { return __generator(this, function (_a) {"),
        "single-line class-field async arrow with no hoisted var must stay inline.\nOutput:\n{output}"
    );
}

// --- Async auto-accessor arrows ------------------------------------------

/// Single-line async auto-accessor arrow that hoists a `var` keeps the callback
/// inline (was wrongly broken multi-line pre-fix).
#[test]
fn auto_accessor_arrow_single_line_with_hoisted_var_stays_inline() {
    let output = emit_es5(
        "class Cls_f { accessor field_f = async () => { var tmp_f = 4; return tmp_f; }; }",
    );

    assert!(
        output.contains("function () { var tmp_f; return __generator(this, function (_a) {"),
        "single-line async auto-accessor arrow must keep the hoisted-var callback inline.\nOutput:\n{output}"
    );
}

/// Multi-line async auto-accessor arrow that hoists a `var` breaks the callback
/// across lines (guards against over-inlining).
#[test]
fn auto_accessor_arrow_multi_line_with_hoisted_var_breaks() {
    let output = emit_es5(
        "class Cls_g { accessor field_g = async () => {\n    var tmp_g = 5;\n    return tmp_g;\n  }; }",
    );

    assert!(
        !output.contains("function () { var tmp_g;"),
        "multi-line async auto-accessor arrow must break the callback across lines.\nOutput:\n{output}"
    );
}
