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

#[test]
fn amd_es5_async_arrow_dynamic_import_uses_wrapper_runtime() {
    let output = emit_wrapped(
        "export const obj = { m: async () => { const req = await import(\"./dep\"); } };\n",
        ModuleKind::AMD,
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            "new Promise(function (resolve_1, reject_1) { require([\"./dep\"], resolve_1, reject_1); }).then(__importStar)"
        ),
        "AMD ES5 async arrows should lower dynamic import through async require.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("yield*/, import(\"./dep\")"),
        "AMD ES5 async arrows should not leave native dynamic import in the generator body.\nOutput:\n{output}"
    );
}

#[test]
fn amd_es5_exported_async_function_dynamic_import_uses_wrapper_runtime() {
    let output = emit_wrapped(
        "export async function f() { const req = await import(\"./dep\"); }\n",
        ModuleKind::AMD,
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            "new Promise(function (resolve_1, reject_1) { require([\"./dep\"], resolve_1, reject_1); }).then(__importStar)"
        ),
        "AMD ES5 exported async functions should lower dynamic import through async require.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(
            "Promise.resolve().then(function () { return __importStar(require(\"./dep\")); })"
        ),
        "AMD ES5 exported async functions should not use the CommonJS dynamic-import branch.\nOutput:\n{output}"
    );
}

#[test]
fn amd_es5_exported_async_method_dynamic_import_uses_wrapper_runtime() {
    let output = emit_wrapped(
        "export class C { async m() { const req = await import(\"./dep\"); } }\n",
        ModuleKind::AMD,
        ScriptTarget::ES5,
    );

    assert!(
        output.contains(
            "new Promise(function (resolve_1, reject_1) { require([\"./dep\"], resolve_1, reject_1); }).then(__importStar)"
        ),
        "AMD ES5 exported async methods should lower dynamic import through async require.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(
            "Promise.resolve().then(function () { return __importStar(require(\"./dep\")); })"
        ),
        "AMD ES5 exported async methods should not use the CommonJS dynamic-import branch.\nOutput:\n{output}"
    );
}

#[test]
fn amd_es5_exported_async_arrow_const_downlevels_to_var() {
    let output = emit_wrapped(
        "export const l = async () => { const req = await import(\"./dep\"); };\n",
        ModuleKind::AMD,
        ScriptTarget::ES5,
    );

    assert!(
        output.contains("var l = function () { return __awaiter("),
        "ES5 exported async-arrow locals should use var before the export assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("const l = function ()"),
        "ES5 exported async-arrow locals should not preserve const after async lowering.\nOutput:\n{output}"
    );
}

#[test]
fn amd_es5_async_dynamic_import_preserves_await_trailing_comment() {
    let output = emit_wrapped(
        "export async function f() {\n    const req = await import(\"./dep\") // ONE\n}\n",
        ModuleKind::AMD,
        ScriptTarget::ES5,
    );

    assert!(
        output.contains("case 0: return [4 /*yield*/, new Promise(function (resolve_1, reject_1) { require([\"./dep\"], resolve_1, reject_1); }).then(__importStar)]; // ONE"),
        "Async ES5 lowering should carry the trailing comment onto the yielded import.\nOutput:\n{output}"
    );
    assert!(
        output.contains("req = _a.sent() // ONE\n"),
        "Async ES5 lowering should carry the trailing comment onto the resumed assignment before the generated semicolon.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\n// ONE\n"),
        "The source trailing comment should not be replayed after the lowered function.\nOutput:\n{output}"
    );
}

#[test]
fn amd_es5_async_dynamic_import_callbacks_are_file_sequenced() {
    let output = emit_wrapped(
        r#"export async function f() { const req = await import("./one"); }
export const obj = { m: async () => { const req = await import("./two"); } };
export class C { async m() { const req = await import("./three"); } }
export class D { p = { m: async () => { const req = await import("./four"); } }; }
"#,
        ModuleKind::AMD,
        ScriptTarget::ES5,
    );

    for (specifier, id) in [("./one", 1), ("./two", 2), ("./three", 3), ("./four", 4)] {
        let expected = format!(
            "new Promise(function (resolve_{id}, reject_{id}) {{ require([\"{specifier}\"], resolve_{id}, reject_{id}); }}).then(__importStar)"
        );
        assert!(
            output.contains(&expected),
            "AMD ES5 async dynamic import callback ids should be file-sequenced.\nExpected: {expected}\nOutput:\n{output}"
        );
    }
    assert!(
        output.contains("m: function () { return __awaiter(_this, void 0, void 0, function () {"),
        "ES5 instance field async arrows should preserve lexical this through _this.\nOutput:\n{output}"
    );
}
