use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn emit_system_es2015(source: &str) -> String {
    let (parser, root) = parse_test_source(source);
    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn lower_emit_module(source: &str, module: ModuleKind, target: ScriptTarget) -> String {
    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        module,
        target,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let emit_plan = LoweringPass::new(&parser.arena, &ctx).run_plan(root);
    let mut printer = Printer::with_emit_plan_and_options(&parser.arena, emit_plan, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

include!("system_emit_parts/part_00.rs");
include!("system_emit_parts/part_01.rs");
