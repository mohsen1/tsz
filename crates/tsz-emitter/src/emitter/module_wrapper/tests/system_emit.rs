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

#[test]
fn system_dotted_namespace_export_folds_outer_namespace_only() {
    let output = emit_system_es2015(
        r#""use strict";
export namespace A.B.C {
    export function foo() {}
}

export function bar() {
    return A.B.C.foo();
}
"#,
    );
    let expected = r#"System.register([], function (exports_1, context_1) {
    "use strict";
    var A;
    var __moduleName = context_1 && context_1.id;
    function bar() {
        return A.B.C.foo();
    }
    exports_1("bar", bar);
    return {
        setters: [],
        execute: function () {
            (function (A) {
                var B;
                (function (B) {
                    var C;
                    (function (C) {
                        function foo() { }
                        C.foo = foo;
                    })(C = B.C || (B.C = {}));
                })(B = A.B || (A.B = {}));
            })(A || (exports_1("A", A = {})));
        }
    };
});
"#;

    assert_eq!(output, expected);
    assert!(
        output.contains("var A;\n    var __moduleName = context_1 && context_1.id;"),
        "System output should hoist the outer dotted namespace binding before functions.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"bar\", bar);"),
        "System output should export the top-level function outside execute.\nOutput:\n{output}"
    );
    assert!(
        output.contains("})(A || (exports_1(\"A\", A = {})));"),
        "Dotted exported namespace should fold only the outer namespace into the System export call.\nOutput:\n{output}"
    );
}

#[test]
fn system_es5_default_class_export_uses_hoisted_assignment_iife() {
    let source = "export default class A { method() { return 42; } }\n";
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::System,
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let emit_plan = LoweringPass::new(&parser.arena, &ctx).run_plan(root);
    let mut printer = Printer::with_emit_plan_and_options(&parser.arena, emit_plan, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("A = /** @class */ (function ()"),
        "System ES5 default class should assign the lowered IIFE to the hoisted binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("A.prototype.method = function ()"),
        "System ES5 default class methods should be downleveled onto the prototype.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"default\", A);"),
        "System default export should publish the lowered class binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("A = class A"),
        "System ES5 default class must not preserve a native class expression.\nOutput:\n{output}"
    );
}

#[test]
fn system_es5_named_and_local_classes_use_assignment_iifes() {
    let source =
        "class Local { local() { return 1; } }\nexport class Named { named() { return 2; } }\n";
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::System,
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let emit_plan = LoweringPass::new(&parser.arena, &ctx).run_plan(root);
    let mut printer = Printer::with_emit_plan_and_options(&parser.arena, emit_plan, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("Local = /** @class */ (function ()")
            && output.contains("Named = /** @class */ (function ()"),
        "System ES5 local and named-export classes should assign lowered IIFEs.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"Named\", Named);"),
        "System named class export should publish the lowered class binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Local = class Local") && !output.contains("Named = class Named"),
        "System ES5 classes must not preserve native class expressions.\nOutput:\n{output}"
    );
}

#[test]
fn umd_dynamic_import_only_file_gets_wrapper_and_loader_branch() {
    let source = r#"class C {
    _path = "./other";
    dynamic() {
        return import(this._path);
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::UMD,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let emit_plan = LoweringPass::new(&parser.arena, &ctx).run_plan(root);
    let mut printer = Printer::with_emit_plan_and_options(&parser.arena, emit_plan, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("(function (factory) {"),
        "Dynamic-import-only UMD files need the wrapper factory.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "var __syncRequire = typeof module === \"object\" && typeof module.exports === \"object\";"
        ),
        "UMD dynamic import needs the runtime branch discriminator.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "return _a = this._path, __syncRequire ? Promise.resolve().then(() => __importStar(require(_a))) : new Promise((resolve_1, reject_1) => { require([_a], resolve_1, reject_1); }).then(__importStar);"
        ),
        "UMD dynamic import should preserve expression evaluation before choosing sync or AMD loading.\nOutput:\n{output}"
    );
}

#[test]
fn umd_es5_class_method_dynamic_import_uses_loader_branch() {
    let source = r#"class C {
    _path = "./other";
    dynamic() {
        return import(this._path);
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::UMD,
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let emit_plan = LoweringPass::new(&parser.arena, &ctx).run_plan(root);
    let mut printer = Printer::with_emit_plan_and_options(&parser.arena, emit_plan, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains(
            "var __syncRequire = typeof module === \"object\" && typeof module.exports === \"object\";"
        ),
        "UMD dynamic import needs the runtime branch discriminator.\nOutput:\n{output}"
    );
    assert!(
        output.contains("C.prototype.dynamic = function () {"),
        "Class method should be lowered through the ES5 class emitter.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "return _a = this._path, __syncRequire ? Promise.resolve().then(function () { return __importStar(require(_a)); }) : new Promise(function (resolve_1, reject_1) { require([_a], resolve_1, reject_1); }).then(__importStar);"
        ),
        "ES5 class method dynamic import should preserve expression evaluation before choosing sync or AMD loading.\nOutput:\n{output}"
    );
}

#[test]
fn umd_import_helpers_declares_tslib_for_class_and_decorator_helpers() {
    let source = r#"import * as dep from "dep";
declare var dec: any;
@dec
export class Derived extends dep.Base {}
"#;
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::UMD,
            target: ScriptTarget::ES5,
            import_helpers: true,
            legacy_decorators: true,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("define([\"require\", \"exports\", \"tslib\", \"dep\"], factory);"),
        "UMD AMD branch should list tslib before value imports when helpers are imported.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var tslib_1 = require(\"tslib\");"),
        "UMD factory body should declare the tslib binding through require().\nOutput:\n{output}"
    );
    assert!(
        output.contains("tslib_1.__extends(Derived, _super);"),
        "UMD ES5 class helper should use the declared tslib binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Derived = tslib_1.__decorate(["),
        "UMD decorator helper should use the declared tslib binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("})(function (require, exports, tslib_1)"),
        "UMD should not rely on a tslib factory parameter that the CJS branch cannot supply.\nOutput:\n{output}"
    );
}

#[test]
fn amd_dynamic_import_only_file_gets_wrapper_and_async_require() {
    let source = r#"const path = "./other";
import(path);
"#;
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::AMD,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("define([\"require\", \"exports\"], function (require, exports) {"),
        "Dynamic-import-only AMD files need the define wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "_a = path, new Promise((resolve_1, reject_1) => { require([_a], resolve_1, reject_1); }).then(__importStar);"
        ),
        "AMD dynamic import should use async require with one eager specifier evaluation.\nOutput:\n{output}"
    );
}

#[test]
fn system_dynamic_import_only_file_uses_context_import() {
    let source = r#"const path = "./other";
import(path);
"#;
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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("System.register([], function (exports_1, context_1) {"),
        "Dynamic-import-only System files need the System.register wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("context_1.import(path);"),
        "System dynamic import should use the wrapper context import hook.\nOutput:\n{output}"
    );
}

#[test]
fn exported_async_function_dynamic_import_keeps_amd_wrapper_kind() {
    let source = r#"export async function load(path: string) {
    return import(path);
}
"#;
    let output = lower_emit_module(source, ModuleKind::AMD, ScriptTarget::ES2015);

    assert!(
        output.contains("define([\"require\", \"exports\"], function (require, exports) {"),
        "Exported async function should stay inside the AMD wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "return _a = path, new Promise((resolve_1, reject_1) => { require([_a], resolve_1, reject_1); }).then(__importStar);"
        ),
        "Dynamic import inside the CJS-export-masked async body should use AMD async require.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Promise.resolve().then(() => __importStar(require(path)))"),
        "AMD wrapper kind must not be erased to the CommonJS dynamic-import form.\nOutput:\n{output}"
    );
}

#[test]
fn exported_async_function_dynamic_import_keeps_system_wrapper_kind() {
    let source = r#"export async function load(path: string) {
    return import(path);
}
"#;
    let output = lower_emit_module(source, ModuleKind::System, ScriptTarget::ES2015);

    assert!(
        output.contains("System.register([], function (exports_1, context_1) {"),
        "Exported async function should stay inside the System wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("context_1.import(path)"),
        "Dynamic import inside the CJS-export-masked async body should use the System context import hook.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("require(path)"),
        "System wrapper kind must not be erased to a require-based dynamic import.\nOutput:\n{output}"
    );
}

#[test]
fn exported_async_function_dynamic_import_keeps_umd_wrapper_kind() {
    let source = r#"export async function load(path: string) {
    return import(path);
}
"#;
    let output = lower_emit_module(source, ModuleKind::UMD, ScriptTarget::ES2015);

    assert!(
        output.contains(
            "var __syncRequire = typeof module === \"object\" && typeof module.exports === \"object\";"
        ),
        "UMD wrapper should keep the dynamic import runtime branch discriminator.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "return _a = path, __syncRequire ? Promise.resolve().then(() => __importStar(require(_a))) : new Promise((resolve_1, reject_1) => { require([_a], resolve_1, reject_1); }).then(__importStar);"
        ),
        "Dynamic import inside the CJS-export-masked async body should use the UMD loader branch.\nOutput:\n{output}"
    );
}

#[test]
fn system_wrapper_elides_unused_value_import_dependency() {
    let source = r#"import * as a from "a";

const value = 1;
"#;
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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("System.register([], function (exports_1, context_1) {"),
        "Unused value imports should not schedule System dependencies.\nOutput:\n{output}"
    );
    assert!(
        output.contains("setters: [],"),
        "Unused value imports should not produce System setters.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\"a\""),
        "Unused value import module specifier should be elided from System output.\nOutput:\n{output}"
    );
}

#[test]
fn system_es5_default_class_uses_class_iife_assignment() {
    let source = r#"export default class A {
    method() {
        return 42;
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("A = /** @class */ (function () {"),
        "System ES5 default class should assign an ES5 class IIFE.\nOutput:\n{output}"
    );
    assert!(
        output.contains("A.prototype.method = function () {"),
        "System ES5 default class methods should be lowered to prototype assignments.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"default\", A);"),
        "System default export should still publish the class binding after assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("A = class A"),
        "System ES5 output must not leave a native class expression.\nOutput:\n{output}"
    );
}

#[test]
fn system_wrapper_keeps_used_namespace_import_dependency() {
    let source = r#"import * as a from "a";

a.run();
"#;
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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("System.register([\"a\"], function (exports_1, context_1) {"),
        "Used namespace imports should remain System dependencies.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a = a_1;"),
        "Used namespace import should receive the setter argument.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a.run();"),
        "Runtime namespace import usage should remain in execute body.\nOutput:\n{output}"
    );
}

#[test]
fn system_es5_named_exported_class_uses_class_iife_assignment() {
    let source = r#"export class A {
    method() {
        return 42;
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("A = /** @class */ (function () {"),
        "System ES5 named class export should assign an ES5 class IIFE.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"A\", A);"),
        "System named export should publish the class binding after assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("A = class A"),
        "System ES5 output must not leave a native class expression.\nOutput:\n{output}"
    );
}

#[test]
fn system_es5_named_exported_class_static_block_runs_after_export() {
    let source = r#"declare function side(x: any): void;
export class A {
    static {
        side(A);
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let assignment_pos = output
        .find("A = /** @class */ (function () {")
        .expect("System ES5 named class export should assign an ES5 class IIFE");
    let export_pos = output
        .find("exports_1(\"A\", A);")
        .expect("System named export should publish the class binding after assignment");
    let static_block_pos = output
        .find("(function () {\n                side(A);\n            })();")
        .expect("System ES5 static block should lower to an IIFE");

    assert!(
        assignment_pos < export_pos && export_pos < static_block_pos,
        "System ES5 named class static block should run after the export call.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("A = (_a ="),
        "Static-block-only class exports should not fold the static block into the assignment RHS.\nOutput:\n{output}"
    );
}

#[test]
fn system_es5_named_exported_class_with_renamed_binding_static_block_runs_after_export() {
    // Class-name-independent variant: static-block ordering applies regardless of identifier spelling.
    let source = r#"declare function side(x: any): void;
export class Cls {
    static {
        side(Cls);
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let assignment_pos = output
        .find("Cls = /** @class */ (function () {")
        .expect("System ES5 named class export should assign an ES5 class IIFE");
    let export_pos = output
        .find("exports_1(\"Cls\", Cls);")
        .expect("System named export should publish the class binding after assignment");
    let static_block_pos = output
        .find("side(Cls);")
        .expect("System ES5 static block should lower to an IIFE");

    assert!(
        assignment_pos < export_pos && export_pos < static_block_pos,
        "System ES5 named class static block should run after the export call.\nOutput:\n{output}"
    );
}

#[test]
fn system_es5_default_exported_class_static_block_runs_before_export() {
    // For `export default class`, tsc emits the static block IIFE before
    // exports_1("default", ...) — opposite of named exports.
    let source = r#"declare function side(x: any): void;
export default class MyClass {
    static {
        side(MyClass);
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let static_block_pos = output
        .find("side(MyClass);")
        .expect("System ES5 static block should lower to an IIFE");
    let export_pos = output
        .find("exports_1(\"default\",")
        .expect("System default export should publish the class binding");

    assert!(
        static_block_pos < export_pos,
        "System ES5 default class static block should run before the export call.\nOutput:\n{output}"
    );
}

#[test]
fn system_es5_anonymous_default_exported_class_static_block_runs_before_export() {
    // Anonymous `export default class` (no identifier): same rule as named default —
    // static block IIFE runs before exports_1("default", ...).
    let source = r#"declare function side(x: any): void;
export default class {
    static {
        side(42);
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let static_block_pos = output
        .find("side(42);")
        .expect("System ES5 static block should lower to an IIFE");
    let export_pos = output
        .find("exports_1(\"default\",")
        .expect("System default export should publish the class binding");

    assert!(
        static_block_pos < export_pos,
        "System ES5 anonymous default class static block should run before the export call.\nOutput:\n{output}"
    );
}

#[test]
fn system_es5_non_exported_class_static_block_runs_after_assignment() {
    // Non-exported classes have no exports_1; the static block must still run
    // after the class assignment (not inside the IIFE before `return`).
    let source = r#"declare function side(x: any): void;
class Internal {
    static {
        side(Internal);
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let assignment_pos = output
        .find("Internal = /** @class */ (function () {")
        .expect("System ES5 non-exported class should assign an ES5 class IIFE");
    let static_block_pos = output
        .find("side(Internal);")
        .expect("System ES5 static block should lower to an IIFE");

    assert!(
        assignment_pos < static_block_pos,
        "System ES5 non-exported class static block should run after the assignment.\nOutput:\n{output}"
    );
    // The static block must not be folded into the assignment expression.
    assert!(
        !output.contains("Internal = (_a ="),
        "Static-block-only classes should not fold the static block into the assignment RHS.\nOutput:\n{output}"
    );
}

#[test]
fn system_wrapper_inlines_const_enum_member_accesses() {
    let source = r#"declare function use(a: any);
const enum TopLevelConstEnum { X }

export function foo() {
    use(TopLevelConstEnum.X);
    use(M.NonTopLevelConstEnum.X);
}

namespace M {
    export const enum NonTopLevelConstEnum { X }
}
"#;
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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("use(0 /* TopLevelConstEnum.X */);"),
        "System wrapper should inline top-level const enum member accesses.\nOutput:\n{output}"
    );
    assert!(
        output.contains("use(0 /* M.NonTopLevelConstEnum.X */);"),
        "System wrapper should inline namespace const enum member accesses.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("use(TopLevelConstEnum.X)")
            && !output.contains("use(M.NonTopLevelConstEnum.X)"),
        "System wrapper must not leave runtime const enum property accesses.\nOutput:\n{output}"
    );
}

#[test]
fn system_wrapper_folds_namespace_and_enum_export_aliases() {
    let source = r#"namespace ns {
    const value = 1;
}

enum AnEnum {
    ONE,
    TWO
}

export { ns, AnEnum, ns as FooBar, AnEnum as BarEnum };
"#;
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
    let output = printer.get_output().to_string();

    assert!(
        output.contains(r#"})(ns || (exports_1("FooBar", exports_1("ns", ns = {}))));"#),
        "System namespace IIFE tail should retain local and aliased exports.\nOutput:\n{output}"
    );
    assert!(
        output
            .contains(r#"})(AnEnum || (exports_1("BarEnum", exports_1("AnEnum", AnEnum = {}))));"#),
        "System enum IIFE tail should retain local and aliased exports.\nOutput:\n{output}"
    );
}

#[test]
fn system_wrapper_folds_direct_exported_dotted_namespace() {
    let source = r#"
export namespace A.B.C {
    export function foo() {}
}

export function bar() {
    return A.B.C.foo();
}
"#;
    let output = emit_system_es2015(source);

    assert!(
        output.contains(r#"})(A || (exports_1("A", A = {})));"#),
        "Direct exported dotted namespaces in System modules should register the root binding through exports_1.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return A.B.C.foo();"),
        "Namespace value references should keep using the local root binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.A = A = {}"),
        "System namespace export folding must not use CommonJS exports assignment.\nOutput:\n{output}"
    );
}

/// `/// <reference .../>` directives should be stripped from JS output.
/// tsc never emits these in JS — they are only preserved in .d.ts files.
#[test]
fn amd_reference_directive_absolute_path_preserved() {
    // References with absolute paths (like JSX lib references) should be
    // emitted before the AMD wrapper, matching tsc behavior.
    let source = r#"/// <reference path="/.lib/react.d.ts" />
import * as React from "react";
export const Foo = () => null;
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("/// <reference path=\"/.lib/react.d.ts\" />"),
        "Absolute-path reference should be emitted before AMD wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("define("),
        "Output should still contain the AMD define() call.\nOutput:\n{output}"
    );
}

/// AMD wrappers should strip relative declaration-file `/// <reference>` directives.
#[test]
fn amd_reference_directive_relative_dts_path_stripped() {
    let source = r#"/// <reference path="file1.d.ts" />
import { x } from "mod";
export const y = x;
"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("/// <reference"),
        "Relative .d.ts reference should be stripped from AMD JS output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("define("),
        "Output should still contain the AMD define() call.\nOutput:\n{output}"
    );
}

#[test]
fn amd_reference_directive_for_bang_module_preserved() {
    let declarations = r#"declare module "http" {
}

declare module 'intern/dojo/node!http' {
import http = require('http');
export = http;
}
"#;
    let source = r#"/// <reference path="a.d.ts"/>

import * as http from 'intern/dojo/node!http';
"#;
    let mut parser = ParserState::new("a.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut declaration_file = parser.arena.source_files[0].clone();
    declaration_file.file_name = "a.d.ts".to_string();
    declaration_file.text = std::sync::Arc::from(declarations);
    declaration_file.is_declaration_file = true;
    parser.arena.source_files.push(declaration_file);

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("/// <reference path=\"a.d.ts\"/>"),
        "Bang module declaration reference should be emitted before AMD wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("define("),
        "Output should still contain the AMD define() call.\nOutput:\n{output}"
    );
}

/// UMD wrappers should also strip `/// <reference>` directives from JS output.
#[test]
fn umd_reference_directive_stripped_from_output() {
    let source = r#"/// <reference path="lib.d.ts" />
import { x } from "mod";
export const y = x;
"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::UMD,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("/// <reference"),
        "Reference directives should be stripped from JS output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(function (factory)"),
        "Output should still contain the UMD wrapper.\nOutput:\n{output}"
    );
}

#[test]
fn system_duplicate_import_temps_follow_source_order() {
    let source = r#"import {A} from "f1";
import {B} from "f2";
import {C} from "f3";
import {D} from "f2";
import {E} from "f2";
import {F} from "f1";

console.log(A + B + C + D + E + F);
"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::System,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var f1_1, f2_1, f3_1, f2_2, f2_3, f1_2;"),
        "System duplicate import temps should follow source order.\nOutput:\n{output}"
    );
}

#[test]
fn system_import_temps_follow_mixed_source_order() {
    let source = r#"const local = "local";
import { value } from "mod";

console.log(local, value);
"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::System,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var local, mod_1;"),
        "System hoists should preserve local/import source order.\nOutput:\n{output}"
    );
}

#[test]
fn system_top_level_using_named_export_keeps_legacy_decorator_assignment_export() {
    let source = "export {};\ndeclare var dec: any;\n@dec\nclass C {}\nexport { C as D };\nusing after = null;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports_1(\"D\", C);"),
        "System named export should preserve the pre-export before __decorate.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"D\", C = __decorate(["),
        "System named export should wrap the legacy decorator reassignment directly.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports_1(\"D\", C);\n            C = __decorate(["),
        "System named export should not split the export from the __decorate reassignment.\nOutput:\n{output}"
    );
}

#[test]
fn system_top_level_using_direct_exported_legacy_class_stays_inline() {
    let source =
        "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nexport class C {}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports_1(\"C\", C = class C {"),
        "System top-level using should keep direct legacy-decorated class exports inline.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"C\", C = __decorate(["),
        "System top-level using should preserve the exported legacy decorator reassignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("});\n                exports_1(\"C\", C);"),
        "System top-level using should not split direct legacy class exports into a trailing export statement.\nOutput:\n{output}"
    );
}

#[test]
fn system_exported_legacy_decorated_class_exports_decorator_assignment() {
    let source = "declare var dec: any;\n@dec\nexport class A {}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __decorate = (this && this.__decorate) || function"),
        "System wrapper should inline __decorate inside the register callback.\nOutput:\n{output}"
    );
    let register_pos = output
        .find("System.register(")
        .expect("System output should include System.register");
    let strict_pos = output[register_pos..]
        .find("\"use strict\";")
        .map(|idx| register_pos + idx)
        .expect("System.register callback should include \"use strict\";");
    let decorate_pos = output
        .find("var __decorate = (this && this.__decorate) || function")
        .expect("System output should include __decorate helper");
    assert!(
        decorate_pos > strict_pos,
        "__decorate helper should be emitted inside the System.register callback after \"use strict\".\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"A\", A);"),
        "System wrapper should preserve the pre-decorator live export.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"A\", A = __decorate(["),
        "System wrapper should export the decorated class reassignment.\nOutput:\n{output}"
    );
}

#[test]
fn system_default_legacy_decorated_class_decorates_before_export() {
    let source = "declare var dec: any;\n@dec\nexport default class Foo {}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let class_pos = output
        .find("Foo = class Foo")
        .expect("System output should assign the default class to Foo");
    let decorate_pos = output
        .find("Foo = __decorate([")
        .expect("System output should preserve the legacy class decorator assignment");
    let export_pos = output
        .find("exports_1(\"default\", Foo);")
        .expect("System output should export the decorated default class value");
    assert!(
        class_pos < decorate_pos && decorate_pos < export_pos,
        "System default class decorators should run before the default export.\nOutput:\n{output}"
    );
}

#[test]
fn system_exported_legacy_decorated_class_aliases_static_self_references() {
    let source = "declare var Something: any;\n@Something({ v: () => Testing123 })\nexport class Testing123 {\n    static prop0: string;\n    static prop1 = Testing123.prop0;\n}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var Testing123_1, Testing123;"),
        "System wrapper should hoist the decorated class self-reference alias.\nOutput:\n{output}"
    );
    let class_pos = output
        .find("Testing123 = Testing123_1 = class Testing123")
        .expect("System output should capture the decorated class value in the alias");
    let export_pos = output
        .find("exports_1(\"Testing123\", Testing123);")
        .expect("System output should preserve the pre-decorator live export");
    let static_pos = output
        .find("Testing123.prop1 = Testing123_1.prop0;")
        .expect("System output should rewrite static self-references to the alias");
    let decorate_pos = output
        .find("exports_1(\"Testing123\", Testing123 = Testing123_1 = __decorate([")
        .expect("System output should export the decorated aliased reassignment");
    assert!(
        class_pos < export_pos && export_pos < static_pos && static_pos < decorate_pos,
        "System decorated class export ordering should match tsc.\nOutput:\n{output}"
    );
}

#[test]
fn system_same_name_legacy_decorated_classes_use_distinct_self_aliases() {
    let source = "declare var Something: any;\n@Something({ v: () => Foo })\nexport class Foo {\n    static prop0: string;\n    static prop1 = Foo.prop0;\n}\ntry {\n    @Something({ v: () => Foo })\n    class Foo {\n        static prop0: string;\n        static prop1 = Foo.prop0;\n    }\n    Foo.prop1;\n}\ncatch (e) {}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var Foo_1, Foo_2, Foo;"),
        "System wrapper should hoist distinct decorated class self-reference aliases before the class binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Foo = Foo_1 = class Foo"),
        "Exported decorated class should use the first alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Foo.prop1 = Foo_1.prop0;"),
        "Exported decorated class static self-reference should use the first alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("let Foo = Foo_2 = class Foo"),
        "Block-scoped same-name decorated class should use the second alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Foo.prop1 = Foo_2.prop0;"),
        "Block-scoped decorated class static self-reference should use the second alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Foo = Foo_2 = __decorate(["),
        "Block-scoped decorator assignment should update the second alias.\nOutput:\n{output}"
    );
}

#[test]
fn system_nested_legacy_decorated_class_emits_decorate_helper() {
    let source = "declare var dec: any;\nexport function make() {\n    @dec\n    class Nested {}\n    return Nested;\n}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __decorate = (this && this.__decorate) || function"),
        "System wrapper should inline __decorate for nested decorated classes.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Nested = __decorate(["),
        "System wrapper should preserve the nested decorated class reassignment.\nOutput:\n{output}"
    );
}

#[test]
fn system_legacy_constructor_param_decorators_emit_param_helper() {
    let source = "declare var dec: any;\n@dec\nclass A {\n    constructor(@dec x: string) {}\n}\nexport { A };\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __param = (this && this.__param) || function"),
        "System wrapper should emit __param when legacy constructor parameter decorators are present.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__param(0, dec)"),
        "System wrapper should preserve constructor parameter decorator calls.\nOutput:\n{output}"
    );
}

#[test]
fn system_legacy_decorator_metadata_emits_metadata_helper() {
    let source =
        "declare var dec: any;\n@dec\nclass A {\n    constructor(x: string) {}\n}\nexport { A };\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            legacy_decorators: true,
            emit_decorator_metadata: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __metadata = (this && this.__metadata) || function"),
        "System wrapper should emit __metadata when decorator metadata is enabled.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:paramtypes\""),
        "System wrapper should emit design:paramtypes metadata for decorated classes with constructors.\nOutput:\n{output}"
    );
}

#[test]
fn system_top_level_using_env_hoists_before_later_nested_var() {
    let source = "export { y };\nusing z = null;\nif (false) {\n    var y = 1;\n}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES2022,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var z, env_1, y;"),
        "System top-level using should place the disposable environment before later nested var hoists.\nOutput:\n{output}"
    );
}

#[test]
fn system_nested_top_level_var_declarations_emit_assignments() {
    let source = "export function read() { return v; }\nfor (let x of []) {\n    let local = x;\n    var v = local;\n}\nfunction keepFunctionVar() {\n    if (true) {\n        var inner = 1;\n    }\n    return inner;\n}\n";

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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var v;"),
        "System wrapper should hoist nested top-level var declarations to the module closure.\nOutput:\n{output}"
    );
    assert!(
        output.contains("let local = x;\n                v = local;"),
        "Nested top-level var initializers should emit as assignments inside execute().\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var v = local;"),
        "Nested top-level var declarations must not redeclare inside execute().\nOutput:\n{output}"
    );
    assert!(
        output.contains("var inner = 1;"),
        "Var declarations inside nested function scopes should remain declarations.\nOutput:\n{output}"
    );
}

#[test]
fn system_exported_object_binding_initializer_assigns_and_exports_hoisted_name() {
    let source = "export let { toString } = 1;\n{\n    let { toFixed } = 1;\n}\n";

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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var toString;"),
        "System wrapper should hoist the exported binding name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"toString\", toString = 1..toString);"),
        "System wrapper should export the destructuring assignment value.\nOutput:\n{output}"
    );
    assert!(
        output.contains("let { toFixed } = 1;"),
        "Nested block-scoped destructuring should remain a declaration.\nOutput:\n{output}"
    );
}

#[test]
fn system_recovered_if_initializerless_export_var_hoists_and_erases_body() {
    let source = "if (true)\nexport const cssExports: CssExports;\nexport default cssExports;\n";

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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var cssExports;"),
        "System wrapper should hoist the recovered exported binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (true) { }"),
        "Initializerless recovered export body should erase to an empty block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"default\", cssExports);"),
        "Default export should read the hoisted local binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.cssExports = ;"),
        "System output should not fall through to invalid CommonJS assignment syntax.\nOutput:\n{output}"
    );
}

#[test]
fn system_recovered_if_initialized_export_var_uses_system_export_binding() {
    let source = "if (true)\nexport var value = 1;\nexport default value;\n";

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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var value;"),
        "System wrapper should hoist the recovered initialized export binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"value\", value = 1);"),
        "Recovered initialized export should use the System live-binding writer.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.value = 1"),
        "System execute output should not use the CommonJS export object.\nOutput:\n{output}"
    );
}

#[test]
fn system_recovered_if_empty_export_binding_uses_planned_temp() {
    let source = "if (true)\nexport const {} = value;\nexport default value;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a, _b;"),
        "Recovered exported empty binding should hoist both planned temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"_b\", _b = _a = value);"),
        "Recovered exported empty binding should use the planned export temp.\nOutput:\n{output}"
    );
}

#[test]
fn system_recovered_if_object_rest_export_uses_planned_temp() {
    let source =
        "if (true)\nexport const { x, ...rest } = { x: 'x', y: 'y' };\nexport default x;\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ESNext,
            no_emit_helpers: true,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a, x, rest;"),
        "Recovered exported object-rest binding should hoist the planned source temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = { x: 'x', y: 'y' }, exports_1(\"x\", x = _a.x), exports_1(\"rest\", rest = __rest(_a, [\"x\"]));"),
        "Recovered exported object-rest binding should reuse the planned source temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ x, ...rest } ="),
        "System output should not emit a raw recovered object-rest assignment pattern.\nOutput:\n{output}"
    );
}

#[test]
fn system_object_binding_initializer_assigns_hoisted_name() {
    let source = "let { toString } = 1;\n{\n    let { toFixed } = 1;\n}\nexport {};\n";

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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var toString;"),
        "System wrapper should hoist the binding name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("toString = 1..toString;"),
        "System wrapper should initialize the hoisted binding from the object property.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports_1(\"toString\""),
        "Non-exported binding should not be exported.\nOutput:\n{output}"
    );
    assert!(
        output.contains("let { toFixed } = 1;"),
        "Nested block-scoped destructuring should remain a declaration.\nOutput:\n{output}"
    );
}

#[test]
fn system_statement_scoped_erased_export_keeps_referenced_binding() {
    let source = "if (true)\nexport const cssExports: CssExports;\nexport default cssExports;\n";

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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var cssExports;"),
        "System wrapper should hoist the statement-scoped exported binding for later exports.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (true)"),
        "System wrapper should preserve the recovered if statement shell.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"default\", cssExports);"),
        "System default export should reference the hoisted local binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports_1(\"cssExports\""),
        "The erased statement-scoped export should not emit its own runtime export call.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.cssExports"),
        "Nested System recovery output must not fall back to CommonJS exports.\nOutput:\n{output}"
    );
}

#[test]
fn system_statement_scoped_erased_export_can_feed_named_export() {
    let source = "if (true)\nexport let value: number;\nexport { value as renamed };\n";

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
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var value;"),
        "System wrapper should hoist the statement-scoped local binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"renamed\", value);"),
        "System named export should publish the hoisted local binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.value"),
        "Nested System recovery output must not fall back to CommonJS exports.\nOutput:\n{output}"
    );
}

#[test]
fn system_exported_object_rest_uses_planned_temp() {
    let source = "export const { x, ...rest } = { x: 'x', y: 'y' };\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ESNext,
            no_emit_helpers: true,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a, x, rest;"),
        "System wrapper should hoist the object-rest temp before exported bindings.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = { x: 'x', y: 'y' }, exports_1(\"x\", x = _a.x), exports_1(\"rest\", rest = __rest(_a, [\"x\"]));"),
        "System execute body should export the planned object-rest assignments.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ x, ...rest } ="),
        "System output should not emit a raw object-rest assignment pattern.\nOutput:\n{output}"
    );
}

#[test]
fn system_preserve_jsx_comments_survive_class_expression_wrapper() {
    use crate::emitter::JsxEmit;

    let source = r#"namespace JSX {}
class Component {
    render() {
        return <div>
            {/* missing */}
            {null/* preserved */}
        </div>;
    }
}
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            module_detection_force: true,
            jsx: JsxEmit::Preserve,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var Component;"),
        "Erased JSX namespace should not be hoisted into the System wrapper.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var JSX"),
        "Type-only namespace should remain erased in System output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("{/* missing */}"),
        "Comment-only JSX expression should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("{null /* preserved */}"),
        "Trailing JSX expression comment should be preserved with tsc spacing.\nOutput:\n{output}"
    );
}

/// Imports whose only textual references are to a type alias or
/// interface of the same name must NOT be retained as runtime imports
/// just because their `PascalCase` name appears as the return type of
/// an async function under ES5. Mirrors the existing guard in
/// `extract_awaiter_promise_constructor`.
/// Devin review: <https://github.com/mohsen1/tsz/pull/2314#discussion_r3176824619>
#[test]
fn amd_es5_type_alias_named_like_import_does_not_force_retention() {
    // The source declares a type alias `Foo` AND imports a value named `Foo`.
    // The async function's return type is `Foo`, but `Foo` is a type alias
    // here, so the import should still be elided (no runtime usage).
    let source = r#"import { Foo } from "lib";
type Foo = string;
async function f(): Foo { return "" as any; }
"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // The AMD dependency list / require call should NOT include "lib"
    // because the only "use" of `Foo` was as a type position. The buggy
    // version falsely treated the type alias as a Promise constructor
    // and kept the import.
    assert!(
        !output.contains("\"lib\""),
        "AMD wrapper should not keep `lib` import when the only use of `Foo` is as a type alias.\nOutput:\n{output}"
    );
}

/// JSX factory imports must not be elided by the AMD/System helper-emission
/// usage check, even when the factory name doesn't textually appear in the
/// source (JSX elements reference it implicitly).
/// Devin review: <https://github.com/mohsen1/tsz/pull/2295#discussion_r3176647570>
#[test]
fn amd_jsx_factory_default_import_kept_in_helpers_check() {
    use crate::emitter::JsxEmit;
    let source = r#"import React from "react";
export const Foo = () => <div/>;
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        jsx: JsxEmit::React,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // The default-import factory ("React") has no textual value usage
    // (only JSX), but because it is a JSX factory we must keep the
    // __importDefault helper definition emitted in the AMD wrapper.
    assert!(
        output.contains("__importDefault"),
        "AMD wrapper should still emit __importDefault helper for JSX factory `React` even without textual value usage.\nOutput:\n{output}"
    );
}

#[test]
fn amd_jsx_factory_named_import_from_pragma_kept_in_helpers_check() {
    use crate::emitter::JsxEmit;
    let source = r#"/** @jsx h */
import { h } from "./renderer";
export const Foo = () => <div/>;
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        jsx: JsxEmit::React,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("\"./renderer\""),
        "AMD wrapper should keep a named import used only as an implicit @jsx factory.\nOutput:\n{output}"
    );
    assert!(
        output.contains("renderer_1.h"),
        "AMD JSX factory call should route through the wrapped import substitution.\nOutput:\n{output}"
    );
}

#[test]
fn system_react_jsx_runtime_dependency_is_wrapped() {
    use crate::emitter::JsxEmit;
    let source = r#"namespace JSX {}
class Component {
render() {
    return <div>{null/* preserved */}</div>;
}
}
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::System,
        jsx: JsxEmit::ReactJsx,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("System.register([\"react/jsx-runtime\"]"),
        "System automatic JSX emit should wrap the synthetic runtime dependency.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var jsx_runtime_1, Component;"),
        "System wrapper should hoist the synthetic JSX runtime binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return _jsx(\"div\", { children: null"),
        "System automatic JSX emit should use the ESM-style JSX helper.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import { jsx as _jsx }"),
        "System automatic JSX emit should not leave an ESM import outside the wrapper.\nOutput:\n{output}"
    );
}

#[test]
fn system_react_jsxdev_runtime_dependency_assigns_file_name() {
    use crate::emitter::JsxEmit;
    let source = r#"namespace JSX {}
class Component {
render() {
    return <div>{null/* preserved */}</div>;
}
}
"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::System,
        jsx: JsxEmit::ReactJsxDev,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("System.register([\"react/jsx-dev-runtime\"]"),
        "System jsxdev emit should wrap the synthetic dev runtime dependency.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var jsx_dev_runtime_1, _jsxFileName, Component;"),
        "System jsxdev emit should hoist the runtime and file-name bindings.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_jsxFileName = \"test.tsx\";"),
        "System jsxdev emit should assign the source file name inside execute().\nOutput:\n{output}"
    );
    assert!(output.contains("return _jsxDEV(\"div\""));
}

#[test]
fn system_react_jsxdev_runtime_dependency_overrides_stale_file_name_cache() {
    use crate::emitter::JsxEmit;
    let source = r#"namespace JSX {}
class Component {
render() {
    return <div>{null}</div>;
}
}
"#;
    let mut parser = ParserState::new("fresh.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::System,
        jsx: JsxEmit::ReactJsxDev,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.jsx_dev_file_name = Some("stale.tsx".to_string());
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("_jsxFileName = \"fresh.tsx\";"),
        "System jsxdev emit should always assign the current source file name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_jsxFileName = \"stale.tsx\";"),
        "System jsxdev emit should not reuse stale _jsxFileName values.\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_emits_local_export_star_helper() {
    let output = emit_system_es2015(r#"export * from "a";"#);

    assert!(
        output.contains("System.register([\"a\"], function (exports_1, context_1) {"),
        "System export-star modules should register the re-export dependency.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function exportStar_1(m) {"),
        "System export-star modules should emit the local export-star helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (n !== \"default\") exports[n] = m[n];"),
        "Pure export-star modules should only skip default without an exclusion map.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exportStar_1(a_1_1);"),
        "The dependency setter should forward namespace members through exportStar_1.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exportedNames_1"),
        "Pure export-star modules should not emit an exclusion map.\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_excludes_local_named_exports() {
    let output = emit_system_es2015(
        r#"export * from "a";
export const x = 1;
"#,
    );

    assert!(
        output.contains("var x;"),
        "The local export should still be hoisted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var exportedNames_1 = {\n        \"x\": true\n    };"),
        "Local named exports should be listed in the export-star exclusion map.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "if (n !== \"default\" && !exportedNames_1.hasOwnProperty(n)) exports[n] = m[n];"
        ),
        "Export-star helper should consult the exclusion map when explicit names exist.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"x\", x = 1);"),
        "The local named export should still be published from execute().\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_default_function_uses_empty_exclusion_map() {
    let output = emit_system_es2015(
        r#"export * from "a";
export default function f() {}
"#,
    );

    assert!(
        output.contains("exports_1(\"default\", f);"),
        "The default function should still be hoisted and exported before the setter block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var exportedNames_1 = {};"),
        "Hoisted default function exports should use tsc's empty export-star exclusion map.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "if (n !== \"default\" && !exportedNames_1.hasOwnProperty(n)) exports[n] = m[n];"
        ),
        "The export-star helper should use the empty map shape for hoisted default functions.\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_excludes_named_reexports_and_namespace_reexports() {
    let output = emit_system_es2015(
        r#"export * from "a";
export { y as renamed } from "b";
export * as ns from "c";
"#,
    );

    assert!(
        output.contains("System.register([\"a\", \"b\", \"c\"], function (exports_1, context_1) {"),
        "System should preserve re-export dependencies in source order.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "var exportedNames_1 = {\n        \"renamed\": true,\n        \"ns\": true\n    };"
        ),
        "Named and namespace re-exports should be excluded from export-star forwarding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exportStar_1(a_1_1);"),
        "The star re-export dependency should call exportStar_1.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"renamed\": b_2_1[\"y\"]"),
        "The named re-export should still be published from its setter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"ns\", c_3_1);"),
        "The namespace re-export should still be published from its setter.\nOutput:\n{output}"
    );
}

#[test]
fn system_export_star_matches_mixed_import_reexport_fixture_shape() {
    let output = emit_system_es2015(
        r#"import * as x from "foo";
import * as y from "bar";
export * from "foo";
export * from "bar";
export {x};
export {y};
import {a1, b1, c1 as d1} from "foo";
export {a2, b2, c2 as d2} from "bar";

x,y,a1,b1,d1;
"#,
    );

    assert!(
        output.contains("var x, y, foo_1;"),
        "The mixed import/re-export fixture should hoist namespace imports and named-import module temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "var exportedNames_1 = {\n        \"x\": true,\n        \"y\": true,\n        \"a2\": true,\n        \"b2\": true,\n        \"d2\": true\n    };"
        ),
        "The exclusion map should include local exports and named re-exports, but not star exports.\nOutput:\n{output}"
    );
    assert!(
        output.contains("x = x_1;") && output.contains("exportStar_1(x_1);"),
        "The foo setter should assign the namespace import and forward star exports.\nOutput:\n{output}"
    );
    assert!(
        output.contains("y = y_1;") && output.contains("exportStar_1(y_1);"),
        "The bar setter should assign the namespace import and forward star exports.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "exports_1({\n                    \"a2\": y_1[\"a2\"],\n                    \"b2\": y_1[\"b2\"],\n                    \"d2\": y_1[\"c2\"]\n                });"
        ),
        "Named re-exports from bar should remain grouped in the setter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"x\", x);") && output.contains("exports_1(\"y\", y);"),
        "Local namespace re-exports should be published from execute().\nOutput:\n{output}"
    );
    assert!(
        output.contains("x, y, foo_1.a1, foo_1.b1, foo_1.c1;"),
        "Named import references should still substitute through the module temp.\nOutput:\n{output}"
    );
}

/// When a source file contains `/// <amd-module name='X'/>`, the
/// `System.register` call must include `"X"` as the first argument, matching tsc behavior for
/// `--module system` with the `amd-module` pragma.
#[test]
fn system_amd_module_name_directive_names_the_register_call() {
    let source = "/// <amd-module name='NamedModule'/>\nexport function foo() {}\n";
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
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("System.register(\"NamedModule\","),
        "amd-module directive must name the System.register call.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function foo() { }"),
        "Exported function should appear inside the System wrapper.\nOutput:\n{output}"
    );
}

/// The `bundled_module_name` printer option also names the `System.register`
/// call (used for out-file bundled output). The `amd-module` directive takes
/// precedence when both are present.
#[test]
fn system_bundled_module_name_option_names_the_register_call() {
    let source = "export function bar() {}\n";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES2015,
            bundled_module_name: Some("BundledModule".to_string()),
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("System.register(\"BundledModule\","),
        "bundled_module_name option must name the System.register call.\nOutput:\n{output}"
    );
}

/// When both `/// <amd-module name='X'/>` and `bundled_module_name` are present,
/// the directive takes precedence (matching tsc behavior for amd-module overriding
/// the bundled name).
#[test]
fn system_amd_module_directive_overrides_bundled_module_name() {
    let source = "/// <amd-module name='DirectiveName'/>\nexport function baz() {}\n";
    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ES2015,
            bundled_module_name: Some("BundledName".to_string()),
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("System.register(\"DirectiveName\","),
        "amd-module directive should take precedence over bundled_module_name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\"BundledName\""),
        "bundled_module_name should be suppressed when amd-module directive is present.\nOutput:\n{output}"
    );
}

#[test]
fn system_module_export_destructuring_baseline_check() {
    // Reproduces tests/cases/compiler/systemModule13.ts
    let output = emit_system_es2015(
        r#"export let [x,y,z] = [1, 2, 3];
export const {a: z0, b: {c: z1}} = {a: true, b: {c: "123"}};
for ([x] of [[1]]) {}
"#,
    );
    println!("systemModule13 output:\n{output}");

    let expected = r#"System.register([], function (exports_1, context_1) {
    "use strict";
    var _a, x, y, z, _b, z0, z1;
    var __moduleName = context_1 && context_1.id;
    return {
        setters: [],
        execute: function () {
            _a = [1, 2, 3], exports_1("x", x = _a[0]), exports_1("y", y = _a[1]), exports_1("z", z = _a[2]);
            _b = { a: true, b: { c: "123" } }, exports_1("z0", z0 = _b.a), exports_1("z1", z1 = _b.b.c);
            for ([x] of [[1]]) { }
        }
    };
});
"#;
    assert_eq!(
        output, expected,
        "System module destructuring exports should match tsc baseline.\nOutput:\n{output}"
    );
}

#[test]
fn system_module_array_export_destructuring_uses_temp() {
    // Minimal test: single exported array binding pattern
    let output = emit_system_es2015("export let [x, y] = [1, 2];\n");
    println!("minimal array destructuring output:\n{output}");
    assert!(
        output.contains("var _a, x, y;"),
        "System module should hoist temp before bound names.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = [1, 2], exports_1(\"x\", x = _a[0]), exports_1(\"y\", y = _a[1])"),
        "System module should use temp to publish each element via exports_1.\nOutput:\n{output}"
    );
}

#[test]
fn system_module_array_export_destructuring_reuses_identifier_source() {
    let output = emit_system_es2015("declare const arr: any;\nexport let [x, y] = arr;\n");
    let expected = r#"System.register([], function (exports_1, context_1) {
    "use strict";
    var x, y;
    var __moduleName = context_1 && context_1.id;
    return {
        setters: [],
        execute: function () {
            exports_1("x", x = arr[0]), exports_1("y", y = arr[1]);
        }
    };
});
"#;
    assert_eq!(
        output, expected,
        "Reusable System module destructuring sources should not allocate an RHS temp.\nOutput:\n{output}"
    );
}

#[test]
fn system_module_nested_object_export_destructuring_reuses_identifier_source() {
    let output =
        emit_system_es2015("declare const obj: any;\nexport const {a: {c}, b: d} = obj;\n");
    let expected = r#"System.register([], function (exports_1, context_1) {
    "use strict";
    var c, d;
    var __moduleName = context_1 && context_1.id;
    return {
        setters: [],
        execute: function () {
            exports_1("c", c = obj.a.c), exports_1("d", d = obj.b);
        }
    };
});
"#;
    assert_eq!(
        output, expected,
        "Nested System module destructuring should publish direct reusable source paths.\nOutput:\n{output}"
    );
}
