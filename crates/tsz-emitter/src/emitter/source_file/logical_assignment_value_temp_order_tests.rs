//! Logical-assignment (`??=`) read-cache value-temp ordering tests.
//!
//! Structural rule: when a lexical environment down-levels a nullish-assignment
//! (`??=`) on a member target, `tsc` mints its read-cache value temp lazily, in
//! evaluation order, from the same per-scope temp counter every other
//! down-leveled temp (optional-chaining, assignment-target, `for..of`) draws
//! from, and declares them all in a *single* `var _a, _b, ...;` in creation
//! order. `tsz` previously (a) pre-reserved the value temp at scope entry — so a
//! preceding optional chain wrongly received the higher numbers — and (b)
//! declared the value temp on a *separate* `var` line. Both diverged from `tsc`.
//!
//! Binder/member names are varied so the assertions track the structural shape,
//! not any spelling.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_at(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
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

/// A hoisted temp must never be split onto a second `var` line: `tsc` declares
/// every down-leveled temp of one lexical environment in a single statement.
fn assert_single_var_line(output: &str) {
    assert!(
        !output.contains("var _a;\nvar "),
        "value temps must share the single hoisted `var`, not a separate line.\nOutput:\n{output}"
    );
}

#[test]
fn optional_chain_before_nullish_assign_numbers_value_temp_last() {
    // Optional chain (statement 1) takes `_a`,`_b`; the `??=` value temp
    // (statement 2) is minted after, so it is `_c` — matching tsc.
    let source = "const d = e?.f?.g?.();\nobj.x ??= 1;\n";
    let output = emit_at(source, ScriptTarget::ES2017);
    assert_single_var_line(&output);
    assert!(
        output.contains("var _a, _b, _c;"),
        "all three temps share one `var` in creation order.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_c = obj.x)"),
        "the `??=` value temp must be the last-minted `_c`.\nOutput:\n{output}"
    );
}

#[test]
fn nullish_assign_before_optional_chain_numbers_value_temp_first() {
    // Reverse order: the `??=` value temp (statement 1) is minted first (`_a`);
    // the optional chain (statement 2) takes `_b`,`_c`.
    let source = "obj.x ??= 1;\nconst d = e?.f?.g?.();\n";
    let output = emit_at(source, ScriptTarget::ES2017);
    assert_single_var_line(&output);
    assert!(
        output.contains("var _a, _b, _c;"),
        "all three temps share one `var` in creation order.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_a = obj.x)") && output.contains("_c = (_b ="),
        "value temp `_a` precedes the optional-chain temps `_b`/`_c`.\nOutput:\n{output}"
    );
}

#[test]
fn renamed_binders_keep_structural_value_temp_order() {
    // Same shape, different member spellings: the decision is structural.
    let source = "const first = alpha?.beta?.gamma?.();\nwidget.slot ??= 7;\n";
    let output = emit_at(source, ScriptTarget::ES2017);
    assert_single_var_line(&output);
    assert!(
        output.contains("var _a, _b, _c;") && output.contains("(_c = widget.slot)"),
        "renamed binders keep the value temp minted last (`_c`).\nOutput:\n{output}"
    );
}

#[test]
fn function_body_merges_value_temp_into_single_var() {
    let source = "function run() { const d = e?.f?.g?.(); obj.x ??= 1; }\n";
    let output = emit_at(source, ScriptTarget::ES2017);
    assert_single_var_line(&output);
    assert!(
        output.contains("var _a, _b, _c;") && output.contains("(_c = obj.x)"),
        "a function body declares its down-leveled temps in one `var`.\nOutput:\n{output}"
    );
}

#[test]
fn multiple_nullish_assign_value_temps_share_one_var_in_order() {
    // Three `??=` in one environment: the value temps are minted in statement
    // order and declared together, `var _a, _b, _c;` — not on separate lines
    // and not pre-reserved out of order.
    let source = "this0.a ??= 1;\nbox.b ??= 2;\nthis0.c ??= 3;\n";
    let output = emit_at(source, ScriptTarget::ES2017);
    assert_single_var_line(&output);
    assert!(
        output.contains("var _a, _b, _c;"),
        "the three value temps share one `var` in creation order.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_a = this0.a)")
            && output.contains("(_b = box.b)")
            && output.contains("(_c = this0.c)"),
        "each value temp keeps its statement-order number.\nOutput:\n{output}"
    );
}
