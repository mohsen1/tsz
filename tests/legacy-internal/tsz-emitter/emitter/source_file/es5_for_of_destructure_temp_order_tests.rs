//! ES5 for-of assignment-target destructuring temp-ordering tests.
//!
//! Structural rule: when an ES5 array-indexing `for-of` statement's target is a
//! destructuring assignment pattern that needs a hoisted source/nested temp, tsc
//! allocates that hoisted destructuring temp (in source order across all sibling
//! for-of statements) BEFORE allocating any for-of loop-control (index/array)
//! temp. tsz reserves those temps in a pre-pass so the global temp numbering
//! matches.
//!
//! These tests vary the binding names and pattern shapes so a fix keyed on a
//! particular identifier or spelling would fail.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_es5(source: &str, downlevel_iteration: bool) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        downlevel_iteration,
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

/// Reported repro shape: two sibling array assignment-target for-of loops over
/// non-identifier iterables. tsc allocates both destructuring source temps
/// (`_a`, `_b`) first, then the loop-control index/array temps (`_i`, `_c`, ...).
#[test]
fn array_assignment_for_of_allocates_destructure_temps_before_loop_control() {
    let source = "var nameA;\n\
         for ([, nameA] of getA()) { nameA; }\n\
         for ([, nameA] of getB()) { nameA; }\n";
    let output = emit_es5(source, false);

    // Both destructuring source temps claim the low numbers up front.
    assert!(
        output.contains("var _a, _b;"),
        "Destructuring source temps must be hoisted first as _a, _b.\nOutput:\n{output}"
    );
    // First loop: index `_i` (special), array `_c` (first auto temp after the
    // reserved destructuring temps), body source temp `_a`.
    assert!(
        output.contains("for (var _i = 0, _c = getA(); _i < _c.length; _i++)"),
        "First loop control temps must follow the destructuring temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = _c[_i], nameA = _a[1]"),
        "First loop source temp should be the first auto temp (_a).\nOutput:\n{output}"
    );
    // Second loop's destructuring source temp is `_b`; its loop-control temps
    // (`_d`, `_e`) come even later.
    assert!(
        output.contains("for (var _d = 0, _e = getB(); _d < _e.length; _d++)"),
        "Second loop control temps must follow all destructuring temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_b = _e[_d], nameA = _b[1]"),
        "Second loop destructuring source temp must be _b.\nOutput:\n{output}"
    );
}

/// Same rule with renamed binding identifiers. If the fix were keyed on the name
/// `nameA`, this differently-named case would diverge.
#[test]
fn array_assignment_for_of_temp_order_is_name_agnostic() {
    let source = "var zzz;\n\
         for ([, zzz] of getA()) { zzz; }\n\
         for ([, zzz] of getB()) { zzz; }\n";
    let output = emit_es5(source, false);

    assert!(
        output.contains("var _a, _b;"),
        "Destructuring source temps must be hoisted first regardless of name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = _c[_i], zzz = _a[1]"),
        "First loop source temp should be _a regardless of binding name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("for (var _d = 0, _e = getB(); _d < _e.length; _d++)"),
        "Loop-control temps must follow the destructuring temps regardless of name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_b = _e[_d], zzz = _b[1]"),
        "Second loop source temp must be _b regardless of binding name.\nOutput:\n{output}"
    );
}

/// Object assignment-target shape: same two-pass ordering applies to
/// `for ({ k: v } of ...)`.
#[test]
fn object_assignment_for_of_allocates_destructure_temps_before_loop_control() {
    // Two object properties so the lowering extracts a source temp.
    let source = "var v0, v1;\n\
         for ({ a: v0, b: v1 } of getA()) { v0; }\n\
         for ({ a: v0, b: v1 } of getB()) { v0; }\n";
    let output = emit_es5(source, false);

    assert!(
        output.contains("var _a, _b;"),
        "Object destructuring source temps must be hoisted first.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = _c[_i], v0 = _a.a, v1 = _a.b"),
        "First object loop source temp should be _a.\nOutput:\n{output}"
    );
    assert!(
        output.contains("for (var _d = 0, _e = getB(); _d < _e.length; _d++)"),
        "Object loop-control temps must follow the destructuring temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_b = _e[_d], v0 = _b.a, v1 = _b.b"),
        "Second object loop source temp should be _b.\nOutput:\n{output}"
    );
}

/// Default-value object shorthand assignment target inside a function expression.
/// The function body opens a fresh temp scope, so the destructuring temp must
/// claim `_a` before the loop-control array temp `_b`.
#[test]
fn function_scoped_for_of_default_allocates_destructure_temp_first() {
    let source = "(function () {\n  var s0;\n  for ({ s0 = 5 } of [{ s0: 1 }]) { }\n});\n";
    let output = emit_es5(source, false);

    assert!(
        output.contains("for (var _i = 0, _b = [{ s0: 1 }]; _i < _b.length; _i++)"),
        "Function-scoped loop array temp must be _b (after destructure temp _a).\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = _b[_i].s0, s0 = _a === void 0 ? 5 : _a"),
        "Function-scoped destructure temp must be _a.\nOutput:\n{output}"
    );
}

/// Negative/fallback: a single-element array assignment target inlines the source
/// expression, so it allocates no extra hoisted destructuring temp — the loop
/// control temps stay at the low numbers.
#[test]
fn single_element_array_assignment_for_of_inlines_without_extra_temp() {
    let source = "var only;\n\
         for ([only] of getA()) { only; }\n\
         for ([only] of getB()) { only; }\n";
    let output = emit_es5(source, false);

    // No hoisted source temp: the value is read inline as `array[index][0]`, so
    // the loop-control temps start at the lowest auto numbers (`_a`, `_b`, ...).
    assert!(
        !output.contains("var _a, _b;"),
        "Single-element array target should not hoist any destructure source temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("for (var _i = 0, _a = getA(); _i < _a.length; _i++)")
            && output.contains("only = _a[_i][0]"),
        "First single-element loop should inline from its array temp _a.\nOutput:\n{output}"
    );
    assert!(
        output.contains("for (var _b = 0, _c = getB(); _b < _c.length; _b++)")
            && output.contains("only = _c[_b][0]"),
        "Second single-element loop control temps start at _b without a source temp.\nOutput:\n{output}"
    );
}
