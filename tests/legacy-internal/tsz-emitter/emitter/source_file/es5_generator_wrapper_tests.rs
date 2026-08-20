//! Down-leveled (`target: ES5`) plain `function*` generator emit parity tests.
//!
//! `tsc` lowers a generator function to a wrapper whose body is the synthesized
//! `return __generator(this, function (_a) { ... })`. Two `tsc` behaviors are
//! covered here:
//!
//! 1. **Single-line wrapper block.** The wrapper body is a synthesized
//!    single-line block, so its braces hug the lone `return __generator(...)`
//!    statement (and any hoisted `var` declarations) even though the inner
//!    state machine spans multiple lines:
//!    `function g() { return __generator(this, function (_a) {` … `}); }`.
//! 2. **Default-parameter prologue.** A default-initialized parameter must
//!    reproduce the ES5 `if (a === void 0) { a = 5; }` prologue (which also
//!    forces the wrapper multi-line, matching `tsc`); the generator IR path
//!    previously dropped these initializers.
//!
//! All differential expectations were verified against bundled `tsc` 6.0.2
//! (`tsc in.ts --target es5`). Binder names are varied to keep the decision
//! structural (no identifier-string special-casing).

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;

fn emit_es5(source: &str) -> String {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn plain_generator_function_wrapper_is_single_line() {
    let output = emit_es5("function* gen() { yield 1; }\n");
    assert!(
        output.contains("function gen() { return __generator(this, function (_a) {"),
        "Plain generator wrapper body should be a single-line block hugging the \
         `return __generator(...)`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("}); }"),
        "Wrapper close should hug: `}}); }}`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function gen() {\n    return __generator"),
        "Wrapper must not be emitted multi-line.\nOutput:\n{output}"
    );
}

#[test]
fn plain_generator_function_expression_wrapper_is_single_line() {
    let output = emit_es5("const make = function* () { yield* [1, 2]; };\n");
    assert!(
        output.contains("function () { return __generator(this, function (_a) {"),
        "Generator function-expression wrapper should be single-line.\nOutput:\n{output}"
    );
}

#[test]
fn generator_wrapper_single_line_is_binder_name_agnostic() {
    // Vary the binder name: the single-line decision is structural, not keyed
    // on any identifier.
    let output = emit_es5("function* zzTop() { yield 7; }\n");
    assert!(
        output.contains("function zzTop() { return __generator(this, function (_a) {"),
        "Renamed generator binder should still emit a single-line wrapper.\nOutput:\n{output}"
    );
}

#[test]
fn generator_wrapper_with_hoisted_var_stays_single_line() {
    // A hoisted `var` declaration is emitted inline inside the hugging braces,
    // matching `tsc`: `function g() { var x; return __generator(...`.
    let output = emit_es5("function* gen() { var x = 1; yield x; }\n");
    assert!(
        output.contains("function gen() { var x; return __generator(this, function (_a) {"),
        "Hoisted-var generator wrapper should stay single-line.\nOutput:\n{output}"
    );
}

#[test]
fn generator_default_parameter_emits_es5_prologue() {
    // The default initializer must reproduce tsc's `if (a === void 0)` prologue
    // (previously dropped), which also makes the wrapper multi-line.
    let output = emit_es5("function* gen(a = 5) { yield a; }\n");
    assert!(
        output.contains("function gen(a) {"),
        "Default-parameter generator should keep the bare parameter name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (a === void 0) { a = 5; }"),
        "Default-parameter prologue must be emitted, not dropped.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function gen(a) { return __generator"),
        "A default-parameter prologue forces the wrapper multi-line (no single-line hug).\nOutput:\n{output}"
    );
}

#[test]
fn generator_multiple_defaults_resolve_in_declaration_order() {
    // A later default may reference an earlier parameter; each gets its own
    // prologue check in order. Binder names varied from the single-default case.
    let output = emit_es5("function* build(p, q = 2, r = p + 1) { yield p + q + r; }\n");
    assert!(
        output.contains("if (q === void 0) { q = 2; }"),
        "Second parameter default prologue missing.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (r === void 0) { r = p + 1; }"),
        "Third parameter default (referencing an earlier param) missing.\nOutput:\n{output}"
    );
}

#[test]
fn generator_method_default_parameter_emits_prologue() {
    // The default-parameter correctness fix also covers generator methods.
    let output = emit_es5("class C { *step(a = 5) { yield a; } }\n");
    assert!(
        output.contains("if (a === void 0) { a = 5; }"),
        "Generator method default-parameter prologue must be emitted.\nOutput:\n{output}"
    );
}
