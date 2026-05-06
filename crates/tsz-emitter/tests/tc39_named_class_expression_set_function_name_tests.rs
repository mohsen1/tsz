//! Regression test for `__setFunctionName` over-emission on TC39-decorated
//! *named* class expressions in ES2022 mode.
//!
//! tsc emits `__setFunctionName(_classThis, "C")` only when the source class
//! expression is *anonymous* and an outer assignment context provides the
//! name. A named class expression (`class C { ... }`) carries its own name
//! through to the engine — the helper is unnecessary and tsc does not emit
//! the static block.
//!
//! tsz was conflating the two cases: when the assignment-context function
//! name happened to coincide with the class's own name (the
//! `export const C = @dec class C {}` shape), it would emit the helper
//! anyway, producing an extra `static { __setFunctionName(_classThis, "C"); }`
//! block on top of `static { _classThis = this; }`.
//!
//! Source: `crates/tsz-emitter/src/transforms/es_decorators.rs`
//! (the `expression_mode && has_class_decorators` branch in
//! `emit_class_decorators_es2022`).

use tsz_common::common::ScriptTarget;
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;
use tsz_parser::parser::ParserState;

fn parse_lower_emit(source: &str, opts: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn tc39_named_class_expression_does_not_emit_set_function_name_static_block() {
    // `class C { ... }` carries its own engine-visible name; the assignment
    // context match is incidental.
    let source = "declare var dec: any;\nexport const C = @dec class C {};\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2022,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("static { _classThis = this; }"),
        "_classThis capture block must still be emitted.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__setFunctionName"),
        "Named class expression should not emit a `__setFunctionName` static block.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_anonymous_class_expression_still_emits_set_function_name_static_block() {
    // `class { ... }` (anonymous) needs the helper so the engine sees the
    // assignment-context name `C` instead of an empty string.
    let source = "declare var dec: any;\nexport const C = @dec class {};\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2022,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("__setFunctionName(_classThis, \"C\")"),
        "Anonymous class expression must thread the inferred name through `__setFunctionName`.\nOutput:\n{output}"
    );
}
