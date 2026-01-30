//! Shared test utilities for emitter parity tests

use crate::emit_context::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions, ScriptTarget};
use crate::lowering_pass::LoweringPass;
use crate::parser::ParserState;

pub fn assert_parity(source: &str, target: ScriptTarget, module: ModuleKind) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options_legacy = PrinterOptions::default();
    options_legacy.target = target;
    options_legacy.module = module;

    let mut printer_legacy = Printer::with_options(arena, options_legacy);
    printer_legacy.set_source_text(source);
    if matches!(target, ScriptTarget::ES3 | ScriptTarget::ES5) {
        printer_legacy.set_target_es5(true);
    }
    printer_legacy.emit(root);
    let output_legacy = printer_legacy.take_output();

    let mut options_new = PrinterOptions::default();
    options_new.target = target;
    options_new.module = module;

    let ctx = EmitContext::with_options(options_new.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    if source.contains("class") && matches!(target, ScriptTarget::ES3 | ScriptTarget::ES5) {
        assert!(
            !transforms.is_empty(),
            "LoweringPass failed to generate transforms for ES5 class"
        );
    }

    let mut printer_new = Printer::with_transforms_and_options(arena, transforms, options_new);
    printer_new.set_source_text(source);
    if matches!(target, ScriptTarget::ES3 | ScriptTarget::ES5) {
        printer_new.set_target_es5(true);
    }
    printer_new.emit(root);
    let output_new = printer_new.take_output();

    let output_legacy_trimmed = output_legacy.trim_end_matches('\n');
    let output_new_trimmed = output_new.trim_end_matches('\n');

    assert_eq!(
        output_legacy_trimmed, output_new_trimmed,
        "\nParity mismatch for source:\n{}\n\nLegacy:\n{}\n\nNew:\n{}",
        source, output_legacy, output_new
    );
}
