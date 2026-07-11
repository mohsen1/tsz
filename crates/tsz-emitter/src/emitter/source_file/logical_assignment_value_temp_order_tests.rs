//! TypeScript 7 logical-assignment value-temp planning tests.
//!
//! Structural rule: for targets below ES2020, TypeScript 7 plans optional-chain
//! temps before logical-assignment read-cache temps, then plans assignment-target
//! reference temps. Read-cache temps and reference temps use separate `var`
//! declarations even though they share the lexical environment.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_es2017(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES2017,
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
fn optional_chain_before_nullish_assign_reserves_value_temp_after_chain() {
    let output = emit_es2017("const d = e?.f?.g?.();\nobj.x ??= 1;\n");
    assert!(
        output.contains("var _a, _b;\nvar _c;") && output.contains("(_c = obj.x)"),
        "TypeScript 7 reserves optional-chain temps before the separate value bucket.\n{output}"
    );
}

#[test]
fn source_order_does_not_move_value_temp_before_planned_chain_temps() {
    let output = emit_es2017("obj.x ??= 1;\nconst d = e?.f?.g?.();\n");
    assert!(
        output.contains("var _a, _b;\nvar _c;") && output.contains("(_c = obj.x)"),
        "TypeScript 7 plans the chain bucket before the logical value bucket.\n{output}"
    );
}

#[test]
fn logical_value_bucket_precedes_computed_target_reference_bucket() {
    let output = emit_es2017("c.foo[side()] ??= value;\n");
    assert!(
        output.contains("var _a;\nvar _b, _c;")
            && output.contains("(_a = (_b = c.foo)[_c = side()])"),
        "read-cache values precede computed-target reference temps.\n{output}"
    );
}

#[test]
fn multiple_value_temps_share_their_own_declaration() {
    let output = emit_es2017("first.a ??= 1;\nsecond.b ??= 2;\nthird.c ??= 3;\n");
    assert!(
        output.contains("var _a, _b, _c;")
            && output.contains("(_a = first.a)")
            && output.contains("(_b = second.b)")
            && output.contains("(_c = third.c)"),
        "logical read-cache temps retain source order inside their bucket.\n{output}"
    );
}

#[test]
fn function_body_keeps_value_and_reference_buckets_separate() {
    let output = emit_es2017("function run() { c.foo[side()] ??= value; }\n");
    assert!(
        output.contains("function run() { var _a; var _b, _c;")
            && output.contains("(_a = (_b = c.foo)[_c = side()])"),
        "function lexical environments preserve the two planned temp buckets.\n{output}"
    );
}
