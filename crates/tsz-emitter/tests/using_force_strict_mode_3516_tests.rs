//! Regression test for issue #3516: when the `using`/`await using` downlevel
//! transform fires, the emitter must include a `"use strict";` prologue so
//! the lowered code runs in strict mode (matching tsc).

use tsz_common::common::{ModuleKind, ScriptTarget};
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
fn script_with_using_declaration_emits_use_strict() {
    let source = "class R { [Symbol.dispose]() {} }\nusing r = new R();\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2022,
        module: ModuleKind::None,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);
    assert!(
        output.starts_with("\"use strict\";") || output.contains("\n\"use strict\";"),
        "Script using `using` must include `\"use strict\";` prologue.\nOutput:\n{output}"
    );
    // Sanity: the using transform must have actually fired, otherwise this
    // test would pass for the wrong reason.
    assert!(
        output.contains("__addDisposableResource"),
        "Expected the using transform to fire.\nOutput:\n{output}"
    );
}

// (Nested `await using` inside a function body is out of scope for the
// issue's repro and is left to a follow-up — `block_has_using_declarations`
// only inspects top-level statements.)

// Sanity: a regular script without using must NOT spontaneously add
// "use strict" — that would be a regression from the existing default.
#[test]
fn script_without_using_keeps_existing_strict_emit_behavior() {
    let source = "var x = 1;\nx;\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2022,
        module: ModuleKind::None,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);
    assert!(
        !output.starts_with("\"use strict\";"),
        "Script without `using` must not gain a `use strict` prologue.\nOutput:\n{output}"
    );
}
