use crate::context::emit::EmitContext;
use crate::emitter::{JsxEmit, ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

/// When moduleDetection=force, a file without any import/export syntax
/// should still be treated as a module and get the CJS __esModule preamble.
#[test]
fn module_detection_force_emits_esmodule_marker() {
    let source = r#"console.log("hello");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        module_detection_force: true,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("Object.defineProperty(exports, \"__esModule\""),
        "moduleDetection=force should emit __esModule marker for non-module file.\nOutput:\n{output}"
    );
}

/// JS files with CommonJS indicators should not get `__esModule`, even when
/// moduleDetection=force made the file an external module.
#[test]
fn js_nested_require_with_module_detection_force_skips_esmodule_marker() {
    let source = r#"{
require("./foo.ts");
import("./foo.ts");
}
"#;

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        module_detection_force: true,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("__esModule"),
        "JS file with nested require should NOT get __esModule.\nOutput:\n{output}"
    );
}

/// Without moduleDetection=force, a file without import/export syntax
/// should NOT get the CJS __esModule preamble.
#[test]
fn no_module_detection_force_skips_esmodule_marker() {
    let source = r#"console.log("hello");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        module_detection_force: false,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("__esModule"),
        "Without moduleDetection=force, non-module file should NOT get __esModule.\nOutput:\n{output}"
    );
}

/// moduleDetection=force should also cause "use strict" to be emitted
/// for CJS modules (since the file is now treated as a module).
#[test]
fn module_detection_force_emits_use_strict_for_cjs() {
    let source = r#"console.log("hello");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        module_detection_force: true,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("\"use strict\""),
        "moduleDetection=force with CJS should emit \"use strict\".\nOutput:\n{output}"
    );
}

#[test]
fn cjs_exported_var_rewrite_respects_function_parameter_shadow() {
    let source = r#"export const obj = { value: 1 };
export function f(obj: { value: number }) {
    return obj;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("return obj;"),
        "Function parameter should shadow the exported variable inside the body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return exports.obj;"),
        "Function parameter should not be rewritten through exports.\nOutput:\n{output}"
    );
}

#[test]
fn malformed_import_numeric_operand_emits_recovered_expression() {
    let source = "import 10;";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("10;"),
        "Malformed import recovery should preserve the numeric operand as a statement.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\n;"),
        "Malformed import recovery should not emit an empty statement.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_reopen_parameter_shadows_prior_exported_name() {
    let source = r#"namespace Foo {
    export function a() {
        return 5;
    }
}
namespace Foo {
    export function c(a: number) {
        return a;
    }
}
export = Foo;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("return a;"),
        "Namespace cross-block export substitution should not rewrite shadowing parameters.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return Foo.a;"),
        "Namespace cross-block export substitution should not qualify a shadowing parameter.\nOutput:\n{output}"
    );
}

#[test]
fn for_of_assignment_object_rest_uses_iteration_temp() {
    let source = r#"let array: { x: number, y: string }[];
for (let { x, ...restOf } of array) {
    [x, restOf];
}
let xx: number;
let rrestOff: { y: string };
for ({ x: xx, ...rrestOff } of array ) {
    [xx, rrestOff];
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("for (let _a of array) {"),
        "Object-rest binding for-of should keep using a loop temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("for (let _b of array) {"),
        "Object-rest assignment for-of should introduce a loop temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("({ x: xx } = _b, rrestOff = __rest(_b, [\"x\"]));"),
        "Object-rest assignment should be emitted inside the loop body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Object.assign({ x: xx }, rrestOff) of array"),
        "Object-rest assignment must not be emitted as an object-spread expression in the for-of header.\nOutput:\n{output}"
    );
}

#[test]
fn es5_arrow_this_capture_skips_multiple_user_bindings() {
    let source = r#"export function make(this: { value: string }) {
  const _this = "first user binding";
  const _this_1 = "second user binding";
  return (() => this.value + ":" + _this + ":" + _this_1)();
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _this_2 = this;"),
        "Arrow lowering should skip both user _this bindings.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return _this_2.value + \":\" + _this + \":\" + _this_1;"),
        "Rewritten lexical this references should use the fresh capture name.\nOutput:\n{output}"
    );
}

#[test]
fn private_field_weakmap_name_avoids_user_binding() {
    let source = r#"const _C_x = "user binding";

class C {
    #x = 1;

    getX() {
        return this.#x;
    }
}

export const result = [new C().getX(), _C_x];
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _C_x_1;"),
        "Private field lowering should skip the real _C_x binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_C_x_1 = new WeakMap()"),
        "WeakMap initialization should use the collision-free helper name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("[new C().getX(), _C_x]"),
        "The user binding must still be referenced by its original name.\nOutput:\n{output}"
    );
}

#[test]
fn nested_classes_preserve_outer_private_name_scope() {
    let source = r#"class A {
    static #x = 5;
    constructor () {
        class B {
            #x = 5;
            constructor() {
                class C {
                    constructor() {
                        A.#x;
                    }
                }
            }
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("__classPrivateFieldGet(_a, _B_x, \"f\")"),
        "Nested class should lower the nearest lexical private name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("A.;"),
        "Private field access should not drop the private identifier.\nOutput:\n{output}"
    );
}

#[test]
fn class_expression_in_loop_uses_block_scoped_private_temps() {
    let source = r#"const array = [];
for (let i = 0; i < 10; ++i) {
    array.push(class C {
        #myField = "hello";
        #method() {}
        get #accessor() { return 42; }
        set #accessor(val) { }
    });
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a;"),
        "Only the class-expression temp should be var-hoisted outside the loop.\nOutput:\n{output}"
    );
    assert!(
        output
            .contains("let _C_instances, _C_myField, _C_method, _C_accessor_get, _C_accessor_set;"),
        "Private backing names should be recreated in the loop block.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _C_instances"),
        "Private backing names should not be var-hoisted outside the loop.\nOutput:\n{output}"
    );
}

#[test]
fn reserved_private_constructor_method_is_not_extracted() {
    let source = r#"class A {
    #constructor() {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::None,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _A_instances, _A_constructor;"),
        "Reserved private constructor should still reserve tsc's helper name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("#constructor() { }"),
        "Reserved private constructor should remain in the class body.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_A_instances = new WeakSet();"),
        "Instance brand WeakSet should still be initialized.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_A_constructor = function"),
        "Reserved private constructor should not be extracted into a helper function.\nOutput:\n{output}"
    );
}

#[test]
fn computed_class_member_private_access_inlines_weakmap_init() {
    let source = r#"let getX: (a: A) => number;

class A {
    #x = 100;
    [(getX = (a: A) => a.#x, "_")]() {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __classPrivateFieldGet ="),
        "Computed member names with private reads should request the helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "[(_A_x = new WeakMap(), getX = (a) => __classPrivateFieldGet(a, _A_x, \"f\"), \"_\")]"
        ),
        "WeakMap initialization should be sequenced inside the computed member name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\n_A_x = new WeakMap();"),
        "WeakMap initialization should not be emitted again after the class.\nOutput:\n{output}"
    );
}

#[test]
fn es5_class_super_parameter_skips_user_binding() {
    let source = r#"class Base {}

const _super = "user binding";

export class Derived extends Base {
  static value = _super;

  method() {
    return _super;
  }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var Derived = /** @class */ (function (_super_1)"),
        "Derived class wrapper should skip the user _super binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__extends(Derived, _super_1);"),
        "__extends should use the generated super parameter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return _super;") && output.contains("Derived.value = _super;"),
        "Source _super references inside the class body should still resolve to the user binding.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_module_temp_skips_user_binding() {
    let source = r#"import { value } from "foo";

const foo_1 = "user binding";

export const result = value + ":" + foo_1;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("const foo_2 = require(\"foo\");"),
        "CommonJS module temp should skip the user foo_1 binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const foo_1 = \"user binding\";"),
        "User binding should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.result = foo_2.value + \":\" + foo_1;"),
        "Imported reads should use the fresh module temp while local reads remain local.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_named_import_substitution_skips_object_property_keys() {
    let source = r#"import { value } from "foo";

const local = { value: "local property" };

export const result = value + ":" + local.value;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("const local = { value: \"local property\" };"),
        "Object literal property key should not be import-substituted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.result = foo_1.value + \":\" + local.value;"),
        "Value references should still be import-substituted.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ foo_1.value:"),
        "CommonJS substitution must not create an invalid property key.\nOutput:\n{output}"
    );
}

#[test]
fn async_arguments_capture_skips_user_binding() {
    let source = r#"export async function f() {
  const arguments_1 = "user binding";
  await 0;
  return [arguments.length, arguments_1];
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var arguments_2 = arguments;"),
        "Async lowering should skip the user arguments_1 binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [arguments_2.length, arguments_1];"),
        "Captured arguments references should use the fresh generated binding.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_import_helpers_tslib_binding_skips_user_binding() {
    let source = r#"const tslib_1 = "user binding";

export async function f() {
  await 0;
  return tslib_1;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        import_helpers: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("const tslib_2 = require(\"tslib\");"),
        "Helper import should skip the user tslib_1 binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const tslib_1 = \"user binding\";"),
        "User binding should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return tslib_2.__awaiter("),
        "Helper call should use the renamed tslib import binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return tslib_1;"),
        "Source reads should still use the user binding.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_import_helpers_jsx_spread_uses_tslib_assign() {
    let source = r#"declare var React: any;
declare var o: any;
export const x = <span {...o} />;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES5,
        jsx: JsxEmit::React,
        import_helpers: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var tslib_1 = require(\"tslib\");"),
        "JSX spread should request the tslib helper import.\nOutput:\n{output}"
    );
    assert!(
        output.contains("React.createElement(\"span\", tslib_1.__assign({}, o))"),
        "ES5 JSX spread should call the imported __assign helper.\nOutput:\n{output}"
    );
}

#[test]
fn async_arguments_capture_skips_parameter_and_pattern_bindings() {
    let source = r#"export async function f({ arguments_1 }: { arguments_1: string }) {
  const [arguments_2] = ["user binding"];
  await 0;
  return [arguments.length, arguments_1, arguments_2];
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var arguments_3 = arguments;"),
        "Async lowering should skip parameter and binding-pattern names.\nOutput:\n{output}"
    );
    assert!(
        output.contains("arguments_3.length"),
        "Captured arguments references should use the binding-pattern-safe name.\nOutput:\n{output}"
    );
}

/// `export default function f()` in CJS should emit `exports.default = f;`
/// BEFORE the function declaration, because JS function declarations are
/// hoisted. This matches tsc's output ordering.
#[test]
fn default_export_function_hoists_export_assignment() {
    let source = "export default function f() { return 1; }\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // exports.default = f; must appear before `function f()`
    let export_pos = output.find("exports.default = f;");
    let func_pos = output.find("function f()");
    assert!(
        export_pos.is_some() && func_pos.is_some(),
        "Should emit both exports.default = f; and function f().\nOutput:\n{output}"
    );
    assert!(
        export_pos.unwrap() < func_pos.unwrap(),
        "exports.default = f; should appear before function f() (hoisting).\nOutput:\n{output}"
    );
}

/// `export namespace F` can merge with `export default function F`.
/// The default export owns the CommonJS export binding, so the namespace IIFE
/// must augment the local function binding rather than assigning `exports.F`.
#[test]
fn default_export_function_namespace_merge_keeps_local_iife_tail() {
    let source = r#"export default function Decl() {
    return 0;
}

export interface Decl {
    p: number;
}

export namespace Decl {
    export var x = 10;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.default = Decl;"),
        "Default function export should still bind exports.default.\nOutput:\n{output}"
    );
    assert!(
        output.contains("})(Decl || (Decl = {}));"),
        "Merged namespace should augment the local function binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.Decl = Decl = {}"),
        "Merged namespace should not create a named CommonJS export binding.\nOutput:\n{output}"
    );
}

/// `export default function func()` with other statements before the
/// function should hoist `exports.default = func;` to the preamble,
/// before all other statements. This matches tsc behavior:
/// ```js
/// "use strict";
/// Object.defineProperty(exports, "__esModule", { value: true });
/// exports.default = func;        // <-- hoisted to preamble
/// var before = func();           // <-- source statement
/// function func() { return func; } // <-- function declaration
/// ```
#[test]
fn default_export_function_hoisted_to_preamble() {
    let source = r#"var before: typeof func = func();
export default function func(): typeof func {
return func;
}
var after: typeof func = func();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // exports.default = func; should be in the preamble (before `var before`)
    let export_pos = output.find("exports.default = func;");
    let before_pos = output.find("var before");
    let func_pos = output.find("function func()");
    assert!(
        export_pos.is_some(),
        "Should emit exports.default = func; in preamble.\nOutput:\n{output}"
    );
    assert!(
        before_pos.is_some(),
        "Should emit var before.\nOutput:\n{output}"
    );
    assert!(
        export_pos.unwrap() < before_pos.unwrap(),
        "exports.default = func; should appear before var before (preamble hoisting).\nOutput:\n{output}"
    );
    assert!(
        export_pos.unwrap() < func_pos.unwrap(),
        "exports.default = func; should appear before function func().\nOutput:\n{output}"
    );
    // Should NOT have a duplicate exports.default = func; at the function's position
    let count = output.matches("exports.default = func;").count();
    assert_eq!(
        count, 1,
        "Should emit exports.default = func; exactly once.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_default_export_function_hoists_export_assignment() {
    let source = "export default 0;\nexport default function() {}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let function_export_pos = output.find("exports.default = default_1;");
    let value_export_pos = output.find("exports.default = 0;");
    let function_pos = output.find("function default_1()");
    assert!(
        function_export_pos.is_some() && value_export_pos.is_some() && function_pos.is_some(),
        "Should emit the hoisted function export, value export, and synthetic function declaration.\nOutput:\n{output}"
    );
    assert!(
        function_export_pos.unwrap() < value_export_pos.unwrap()
            && value_export_pos.unwrap() < function_pos.unwrap(),
        "Anonymous default function export should be hoisted before the earlier default expression assignment.\nOutput:\n{output}"
    );
    assert_eq!(
        output.matches("exports.default = default_1;").count(),
        1,
        "Should emit the anonymous default function export assignment once.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_default_class_avoids_user_default_1_binding() {
    let source = r#"
const default_1 = "user binding";

export default class {
  value = default_1;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("class default_2"),
        "anonymous default class should avoid colliding with user default_1.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.default = default_2;"),
        "default export should reference the non-colliding synthetic class name.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_default_function_avoids_user_default_1_binding() {
    let source = r#"
const default_1 = "user binding";

export default function () {
  return default_1;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.default = default_2;"),
        "anonymous default function export should reference the non-colliding synthetic name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function default_2()"),
        "anonymous default function declaration should avoid colliding with user default_1.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_default_function_ignores_default_1_in_string_literal() {
    let source = r#"
const label = "default_1";

export default function () {
  return label;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.default = default_1;"),
        "string literal text should not reserve the anonymous default binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function default_1()"),
        "anonymous default function should keep the first synthetic name.\nOutput:\n{output}"
    );
}

/// Non-default function exports should NOT have the export hoisted before
/// the function — they are handled in the preamble instead.
#[test]
fn named_export_function_not_hoisted() {
    let source = "export function g() { return 2; }\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // For named exports, the preamble emits `exports.g = g;` before the
    // function, and there's no second assignment after.
    let preamble_pos = output.find("exports.g = g;");
    let func_pos = output.find("function g()");
    assert!(
        preamble_pos.is_some() && func_pos.is_some(),
        "Should emit both exports.g = g; and function g().\nOutput:\n{output}"
    );
    assert!(
        preamble_pos.unwrap() < func_pos.unwrap(),
        "Preamble exports.g = g; should appear before function g().\nOutput:\n{output}"
    );
}

/// `export { f }` where `f` is a function declaration should emit
/// `exports.f = f;` in the preamble (hoisted) and NOT emit a duplicate
/// assignment at the `export { f }` statement position.
#[test]
fn named_export_specifier_for_function_hoisted() {
    let source = r#"function isValid(x: unknown): x is string {
return typeof x === "string";
}
export { isValid };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // The preamble should contain `exports.isValid = isValid;`
    assert!(
        output.contains("exports.isValid = isValid;"),
        "Should emit hoisted exports.isValid = isValid; in preamble.\nOutput:\n{output}"
    );
    // Should NOT contain `exports.isValid = void 0;`
    assert!(
        !output.contains("exports.isValid = void 0"),
        "Function export should NOT get void 0 initialization.\nOutput:\n{output}"
    );
    // The hoisted assignment should appear before the function body
    let export_pos = output.find("exports.isValid = isValid;").unwrap();
    let func_pos = output.find("function isValid(").unwrap();
    assert!(
        export_pos < func_pos,
        "exports.isValid = isValid; should appear before function isValid().\nOutput:\n{output}"
    );
    // Should only appear once (no duplicate from the inline export { } handler)
    let count = output.matches("exports.isValid = isValid;").count();
    assert_eq!(
        count, 1,
        "exports.isValid = isValid; should appear exactly once.\nOutput:\n{output}"
    );
}

#[test]
fn named_export_specifier_for_undefined_only_uses_preamble() {
    let source = "var undefined;\nexport { undefined };\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.undefined = void 0;"),
        "undefined export should be initialized in the preamble.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var undefined;"),
        "local undefined declaration should still be emitted.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.undefined = undefined;"),
        "undefined self-export should not emit a post-declaration assignment.\nOutput:\n{output}"
    );
}

#[test]
fn repeated_named_export_specifiers_defer_all_aliases_until_const_declaration() {
    let source = "export { x };\nexport { x as xx };\nexport default x;\nconst x = 'x';\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let default_pos = output
        .find("exports.default = x;")
        .expect("default export should emit");
    let decl_pos = output.find("const x = 'x';").expect("const should emit");
    let x_export_pos = output.find("exports.x = x;").expect("x export should emit");
    let xx_export_pos = output
        .find("exports.xx = x;")
        .expect("xx export should emit");

    assert!(
        default_pos < decl_pos && decl_pos < x_export_pos && x_export_pos < xx_export_pos,
        "Named export aliases for a const should emit after the declaration, preserving alias order.\nOutput:\n{output}"
    );
}

/// `export { f as g }` where `f` is a function should still hoist
/// the export with the exported name `g` in the preamble.
#[test]
fn named_export_specifier_aliased_function_hoisted() {
    let source = r#"function impl() { return 42; }
export { impl as myFunc };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // The preamble should contain `exports.myFunc = impl;`
    // (using the local name `impl`, not the exported alias `myFunc` — tsc behavior)
    assert!(
        output.contains("exports.myFunc = impl;"),
        "Should emit hoisted exports.myFunc = impl; in preamble.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.myFunc = void 0"),
        "Aliased function export should NOT get void 0.\nOutput:\n{output}"
    );
}

#[test]
fn named_export_specifier_for_ambient_const_skips_runtime_assignment() {
    let source = "declare const _await: any;\nexport { _await as await };\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.await = void 0;"),
        "Ambient named export should keep the preamble initialization.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.await = _await;"),
        "Ambient named export should not emit a runtime assignment.\nOutput:\n{output}"
    );
}

#[test]
fn inline_cjs_export_skips_initializerless_vars() {
    let source = "export var eVar1, eVar2 = 10;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.eVar2 = exports.eVar1 = void 0;"),
        "Initializerless export should stay in the CJS preamble.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.eVar2 = 10;"),
        "Initialized export should be emitted inline.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var eVar1, eVar2 = 10;"),
        "Mixed export var declarations should not keep a redundant local declaration.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.eVar1 = eVar1;"),
        "Initializerless export should not get a trailing self-assignment.\nOutput:\n{output}"
    );
}

#[test]
fn transformed_class_expression_var_export_emits_inline_assignment() {
    let source = "export var noPrivates = class {\n    static getTags() { }\n    tags() { }\n    private static ps = -1;\n    private p = 12;\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.noPrivates = (_a = class {"),
        "Transformed class expression export should inline the comma expression.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var noPrivates ="),
        "Transformed class expression export should not keep a redundant local binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.noPrivates = noPrivates;"),
        "Transformed class expression export should not need a trailing self-assignment.\nOutput:\n{output}"
    );
}

/// Merged enum declarations in ESM should only have `export` on the first
/// declaration's `var` statement. Subsequent IIFEs should be bare.
#[test]
fn merged_enum_esm_no_spurious_export() {
    let source = r#"export enum Animals {
Cat = 1
}
export enum Animals {
Dog = 2
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // First IIFE should be preceded by `export var Animals;`
    assert!(
        output.contains("export var Animals;"),
        "First enum should have `export var Animals;`.\nOutput:\n{output}"
    );

    // Second IIFE should NOT be preceded by `export`
    // Count occurrences of "export" — should be exactly 1 (on the var decl)
    let export_count = output.matches("export ").count();
    assert_eq!(
        export_count, 1,
        "Should have exactly one `export` (on the var declaration), not on subsequent IIFEs.\nOutput:\n{output}"
    );
}

/// Merged namespace declarations in ESM should only have `export` on the
/// first var declaration, not on subsequent IIFEs.
#[test]
fn merged_namespace_esm_no_spurious_export() {
    let source = r#"export function F() { }
export namespace F {
export var x = 1;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // The namespace IIFE `(function (F) {...})(F || (F = {}))` should NOT
    // be preceded by `export`.
    assert!(
        !output.contains("export (function"),
        "Merged namespace IIFE should not be preceded by `export`.\nOutput:\n{output}"
    );
}

/// When a class has legacy decorators and is exported in CJS, the
/// `exports.X = X;` pre-assignment should appear exactly once — from
/// `emit_legacy_class_decorator_assignment`, NOT also from the
/// `pending_commonjs_class_export_name` path.
#[test]
fn decorated_class_export_no_duplicate_exports() {
    let source = "declare var dec: any;\n@dec export class A {}\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        legacy_decorators: true,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // Count occurrences of `exports.A = A;`
    let count = output.matches("exports.A = A;").count();
    assert_eq!(
        count, 1,
        "exports.A = A; should appear exactly once (pre-assignment before __decorate), \
         not duplicated.\nOutput:\n{output}"
    );
    // The __decorate assignment should also reference exports.A
    assert!(
        output.contains("exports.A = A = __decorate("),
        "Should contain the decorator assignment.\nOutput:\n{output}"
    );
}

#[test]
fn cjs_exported_class_with_mixin_heritage_exports_after_outer_class() {
    let source = r#"export const Mixin = null as any;
export class Base {}
export class XmlElement2 extends Mixin(
    [Base],
    (base: any) => {
        class XmlElement2 extends base {
            num = 0;
        }
        return XmlElement2;
    }) { }
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let return_pos = output
        .find("return XmlElement2;")
        .expect("mixin callback should return the local class");
    let export_pos = output
        .find("exports.XmlElement2 = XmlElement2;")
        .expect("outer class export assignment should be emitted");

    assert!(
        return_pos < export_pos,
        "Outer class export assignment must not be emitted inside the mixin callback.\nOutput:\n{output}"
    );
    let outer_close_pos = output
        .find("}) {\n}\nexports.XmlElement2 = XmlElement2;")
        .expect("outer class should close before its export assignment");
    assert!(
        return_pos < outer_close_pos && outer_close_pos <= export_pos,
        "Outer class export assignment should follow the complete class declaration.\nOutput:\n{output}"
    );
}

/// When `export = f` is present with `export function f()`, the hoisted
/// `exports.f = f;` preamble should be suppressed because `module.exports = f`
/// replaces the entire exports object.
#[test]
fn export_assignment_suppresses_hoisted_func_export() {
    let source = "export function f() { }\nexport = f;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("exports.f = f;"),
        "Hoisted exports.f = f; should be suppressed when export = is present.\nOutput:\n{output}"
    );
    assert!(
        output.contains("module.exports = f;"),
        "module.exports = f; should be present for export =.\nOutput:\n{output}"
    );
}

#[test]
fn export_assignment_preserves_declared_namespace_runtime_value() {
    let source = r#"declare namespace M1 {
    export var a: string;
    export function b(): number;
}
export = M1;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("module.exports = M1;"),
        "Declared namespace export assignment should emit a CommonJS export.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Object.defineProperty(exports, \"__esModule\""),
        "CommonJS export assignment should suppress the __esModule marker.\nOutput:\n{output}"
    );
}

/// When `export = B` is present alongside `export class C {}`, the
/// `exports.C = void 0;` initialization should still be emitted (tsc behavior),
/// but hoisted function exports should be suppressed.
#[test]
fn export_assignment_keeps_void_zero_init_for_classes() {
    let source = "export class C {}\nexport = B;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.C = void 0;"),
        "exports.C = void 0; should be emitted even with export =.\nOutput:\n{output}"
    );
    assert!(
        output.contains("module.exports = B;"),
        "module.exports = B; should be present.\nOutput:\n{output}"
    );
}

/// A file using `import.meta` (with no import/export syntax) should be
/// treated as a module and get the CJS __esModule preamble. `import.meta`
/// is ESM-only syntax, making the file implicitly a module.
#[test]
fn import_meta_triggers_esmodule_marker() {
    let source = r#"const url = import.meta.url;
console.log(url);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("Object.defineProperty(exports, \"__esModule\""),
        "File with import.meta should emit __esModule marker.\nOutput:\n{output}"
    );
}

/// A file without any module syntax or import.meta should NOT get __esModule.
#[test]
fn no_import_meta_no_esmodule_marker() {
    let source = r#"const x = 1;
console.log(x);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("__esModule"),
        "File without module syntax should NOT get __esModule marker.\nOutput:\n{output}"
    );
}

#[test]
fn system_reexport_setter_uses_bracket_access() {
    let source = r#"export { b } from "./b";
export { default as Foo } from "./b";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::System,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("\"b\": b_1_1[\"b\"]"),
        "System re-export setter should read named exports with bracket access.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"Foo\": b_1_1[\"default\"]"),
        "System re-export setter should read default with bracket access.\nOutput:\n{output}"
    );
}

#[test]
fn interface_var_member_recovery_emits_var_statement() {
    let source = "interface Foo<T> {\n    var x: T<>;\n}";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var x;"),
        "Malformed var members in interfaces should recover as a variable statement.\nOutput:\n{output}"
    );
}

#[test]
fn exported_alias_to_uninstantiated_namespace_is_elided() {
    let source = r#"export namespace a {
    export namespace b {
        export interface I {
            foo();
        }
    }
}

export import b = a.b;
export var x: b.I;
x.foo();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.x = void 0;"),
        "Exported variable should still get the CJS-style initializer.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.b"),
        "Exported aliases to type-only namespaces should not emit runtime export assignments.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_parameter_decorator_metadata_preserves_return_type_import() {
    let source = r#"import { Observable } from "./observable";
declare function whatever(a: any, b: any, c: any): void;
class Test {
    foo(@whatever arg: string): Observable<string> {
        return null!;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2020,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __metadata ="),
        "Parameter-decorated method metadata should request the __metadata helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const observable_1 = require(\"./observable\");"),
        "Metadata return type should keep the CommonJS import.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:returntype\", observable_1.Observable)"),
        "Metadata return type should use the CommonJS import substitution.\nOutput:\n{output}"
    );
}

#[test]
fn script_import_equals_to_interface_preserves_alias_emit() {
    let source = "interface I { id: number; }\nimport i = I;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var i = I;"),
        "Top-level script import-equals aliases should be preserved even when the target is type-only.\nOutput:\n{output}"
    );
}
