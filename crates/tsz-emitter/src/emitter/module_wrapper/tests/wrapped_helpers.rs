use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_wrapped(source: &str, module: ModuleKind, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

#[test]
fn amd_async_dynamic_import_helpers_emit_before_define_wrapper() {
    let output = emit_wrapped(
        "export async function f() { await import(\"./dep\"); }\n",
        ModuleKind::AMD,
        ScriptTarget::ES2015,
    );
    let helper_pos = output
        .find("var __awaiter")
        .expect("awaiter helper should be emitted");
    let define_pos = output
        .find("define([\"require\", \"exports\"],")
        .expect("AMD wrapper should be emitted");

    assert!(
        helper_pos < define_pos,
        "AMD runtime helpers should be emitted before the wrapper.\nOutput:\n{output}"
    );
    assert!(
        !output[define_pos..].contains("var __awaiter"),
        "AMD wrapper body should not re-emit runtime helpers.\nOutput:\n{output}"
    );
}

#[test]
fn umd_es5_async_dynamic_import_helpers_emit_before_factory_wrapper() {
    let output = emit_wrapped(
        "export async function f() { await import(\"./dep\"); }\n",
        ModuleKind::UMD,
        ScriptTarget::ES5,
    );
    let awaiter_pos = output
        .find("var __awaiter")
        .expect("awaiter helper should be emitted");
    let generator_pos = output
        .find("var __generator")
        .expect("generator helper should be emitted");
    let wrapper_pos = output
        .find("(function (factory)")
        .expect("UMD wrapper should be emitted");

    assert!(
        awaiter_pos < wrapper_pos && generator_pos < wrapper_pos,
        "UMD ES5 async helpers should be emitted before the wrapper.\nOutput:\n{output}"
    );
    assert!(
        !output[wrapper_pos..].contains("var __awaiter")
            && !output[wrapper_pos..].contains("var __generator"),
        "UMD wrapper body should not re-emit runtime helpers.\nOutput:\n{output}"
    );
}
