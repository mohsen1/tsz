use crate::context::emit::EmitContext;
use crate::emitter::{JsxEmit, ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

use super::parse_test_source;

#[test]
fn commonjs_unused_classic_jsx_factory_name_elides_namespace_import_without_jsx() {
    let source = r#"import * as React from "react";
export var x = 1;
"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        jsx: JsxEmit::React,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("require(\"react\")") && !output.contains("__importStar"),
        "Unused namespace import named like a JSX factory should be elided when the file has no JSX.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.x = 1;"),
        "The exported value should still emit normally.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_classic_jsx_factory_namespace_import_survives_with_jsx() {
    let source = r#"import * as React from "react";
export const x = <div />;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        jsx: JsxEmit::React,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("require(\"react\")") && output.contains("React.createElement"),
        "Namespace import used as the implicit JSX factory must be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_classic_jsx_default_import_ignores_type_only_named_imports() {
    let source = r#"import React, { ComponentPropsWithRef, ElementType, ReactNode } from "react";

type ButtonBaseProps<T extends ElementType> = ComponentPropsWithRef<T> & { children?: ReactNode };

export function Component<T extends ElementType = "span">(props: ButtonBaseProps<T>) {
    return <></>;
}
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        jsx: JsxEmit::React,
        es_module_interop: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __importDefault = "),
        "Classic JSX default factory import should request __importDefault.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__importStar"),
        "Type-only named imports beside a JSX default factory must not force __importStar.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const react_1 = __importDefault(require(\"react\"));"),
        "Default import should lower as default-only when named imports are type-only.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return react_1.default.createElement(react_1.default.Fragment, null);"),
        "Classic JSX should still use the default import as the factory root.\nOutput:\n{output}"
    );
}

#[test]
fn esmodule_es5_default_class_exports_after_iife() {
    let source = r#"export default class A {
    method() { return 1; }
}
"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::ES2015,
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
        output.contains("var A = /** @class */ (function ()"),
        "ES5 default class should be lowered to a local IIFE binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export default A;"),
        "Native ESM default export should be scheduled after the ES5 class binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export default var"),
        "Default export must not prefix the lowered `var` declaration.\nOutput:\n{output}"
    );
}

#[test]
fn esmodule_es5_anonymous_default_class_gets_synthetic_binding() {
    let source = r#"export default class {
    method() { return 1; }
}
"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::ES2015,
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
        output.contains("var default_1 = /** @class */ (function ()"),
        "Anonymous ES5 default class should receive the tsc-style synthetic binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export default default_1;"),
        "Native ESM anonymous default class export should use the synthetic binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export default \n"),
        "Default export must not be left without an emitted expression.\nOutput:\n{output}"
    );
}

/// When moduleDetection=force, a file without any import/export syntax
/// should still be treated as a module and get the CJS __esModule preamble.
#[test]
fn module_detection_force_emits_esmodule_marker() {
    let source = r#"console.log("hello");"#;

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

#[test]
fn commonjs_type_only_reexport_skips_void_zero_preamble() {
    let source = r#"export { I, I as II } from "./ambient";"#;

    let (parser, root) = parse_test_source(source);
    let source_file = parser
        .arena
        .get_source_file(parser.arena.get(root).expect("root node must exist"))
        .expect("source file must exist");

    let mut type_only_nodes = rustc_hash::FxHashSet::default();
    for &stmt_idx in &source_file.statements.nodes {
        let Some(stmt) = parser.arena.get(stmt_idx) else {
            continue;
        };
        let Some(export_decl) = parser.arena.get_export_decl(stmt) else {
            continue;
        };
        let Some(clause_node) = parser.arena.get(export_decl.export_clause) else {
            continue;
        };
        let Some(named_exports) = parser.arena.get_named_imports(clause_node) else {
            continue;
        };
        type_only_nodes.extend(named_exports.elements.nodes.iter().copied());
    }

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        type_only_nodes: std::sync::Arc::new(type_only_nodes),
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("exports.I") && !output.contains("exports.II"),
        "Type-only re-exports should not be preinitialized.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_export_star_does_not_emit_commonjs_reexport_helpers() {
    let source = r#"class Aaa {
}
namespace Aaa {
export class SomeType {
}
}
namespace Bbb {
export class SomeType {
}
export * from Aaa;
}
"#;
    let (parser, root) = parse_test_source(source);

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
        !output.contains("__exportStar") && !output.contains("__createBinding"),
        "Namespace-scoped export star should be erased in JS and should not request CommonJS re-export helpers.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Bbb.SomeType = SomeType;"),
        "Namespace value members should still emit normally.\nOutput:\n{output}"
    );
}

/// moduleDetection=force should also cause "use strict" to be emitted
/// for CJS modules (since the file is now treated as a module).
#[test]
fn module_detection_force_emits_use_strict_for_cjs() {
    let source = r#"console.log("hello");"#;

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
fn namespace_local_var_shadows_module_and_namespace_export_rewrites() {
    let source = r#"export const Something = 2;
export namespace A {
    export namespace B {
        const Something = require("fs").Something;
        const thing = new Something();
        export { thing };
    }
}
"#;

    for target in [ScriptTarget::ES2015, ScriptTarget::ES5] {
        let (parser, root) = parse_test_source(source);
        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            target,
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
            output.contains("new Something()"),
            "Namespace-local variable declarations should shadow same-named module and namespace exports for target {target:?}.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("new exports.Something()") && !output.contains("new B.Something()"),
            "Local constructor references must not be rewritten through export objects for target {target:?}.\nOutput:\n{output}"
        );
    }
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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
fn static_private_initialization_precedes_lowered_static_fields() {
    let source = r#"// https://github.com/microsoft/TypeScript/issues/44113
class C {
    static #qux = 42;
    static ["bar"] = "test";
}
"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let var_pos = output
        .find("var _a, _C_qux;")
        .expect("expected private temp var declaration");
    let comment_pos = output
        .find("// https://github.com/microsoft/TypeScript/issues/44113")
        .expect("expected preserved leading comment");
    let class_pos = output.find("class C").expect("expected class declaration");
    let private_init_pos = output
        .find("_C_qux = { value: 42 };")
        .expect("expected static private initialization");
    let static_field_pos = output
        .find("Object.defineProperty(C, \"bar\"")
        .expect("expected lowered static field");

    // tsc places the file-leading comment before any helpers/hoists, then
    // emits the temp `var _a, _C_qux;` between the comment and the class.
    assert!(
        comment_pos < var_pos,
        "Leading file comment should precede the temp-var hoist.\nOutput:\n{output}"
    );
    assert!(
        var_pos < class_pos,
        "Private temp vars should precede the class declaration.\nOutput:\n{output}"
    );
    assert!(
        private_init_pos < static_field_pos,
        "Static private state should initialize before lowered static fields.\nOutput:\n{output}"
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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
fn es5_classic_jsx_spread_child_lowers_create_element_args() {
    let source = r#"declare var React: any;
declare var items: any;
export const x = <div>{...items}</div>;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES5,
        jsx: JsxEmit::React,
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
        output.contains("var __spreadArray = "),
        "ES5 JSX spread children should request the __spreadArray helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "React.createElement.apply(React, __spreadArray([\"div\", null], items, false))"
        ),
        "Classic JSX spread children should lower createElement args through apply.\nOutput:\n{output}"
    );
}

#[test]
fn es5_classic_jsx_spread_child_preserves_adjacent_children() {
    let source = r#"declare var React: any;
declare var items: any;
export const x = <div>{1}{...items}{2}</div>;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES5,
        jsx: JsxEmit::React,
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
        output.contains(
            "React.createElement.apply(React, __spreadArray(__spreadArray([\"div\", null, 1], items, false), [2], false))"
        ),
        "Classic JSX spread children should preserve regular children around the spread.\nOutput:\n{output}"
    );
}

#[test]
fn es5_automatic_jsx_spread_child_uses_jsxs_array_child() {
    let source = r#"declare var items: any;
export const x = <div>{...items}</div>;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES5,
        jsx: JsxEmit::ReactJsx,
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
        output.contains("var jsx_runtime_1 = require(\"react/jsx-runtime\");"),
        "ES5 automatic JSX runtime imports should be var declarations.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "(0, jsx_runtime_1.jsxs)(\"div\", { children: __spreadArray([], items, true) })"
        ),
        "Automatic JSX spread children should force jsxs with an ES5 array-spread child.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_exported_destructuring_uses_binding_access_paths() {
    let source = r#"'use strict'
// exported destructuring should read from the pattern source
export let [bar1] = [1];
export const { a: bar2 } = { a: 2 };
"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("// exported destructuring should read from the pattern source"),
        "Leading comments before folded CommonJS exports should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.bar1 = [1][0];"),
        "Array binding exports should read by element index.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("[1].bar1"),
        "Array binding exports must not use the binding name as a property.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.bar2 = { a: 2 }.a;"),
        "Object binding exports should read by property name.\nOutput:\n{output}"
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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

#[test]
fn commonjs_default_export_identifier_uses_export_binding_for_exported_var() {
    let source = r#"export const cssExports = 1;
export default cssExports;
"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("exports.default = exports.cssExports;"),
        "Default export should read the CommonJS export binding for exported variables.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.default = cssExports;"),
        "Default export should not read a missing or stale local variable binding.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_default_export_identifier_uses_recovered_nested_export_binding() {
    let source = r#"type CssExports = {};
if (true)
export const cssExports: CssExports;
export default cssExports;
"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("if (true) { }"),
        "Recovered no-initializer export should leave an empty control-flow body.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.default = exports.cssExports;"),
        "Default export should read the recovered CommonJS export binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.default = cssExports;"),
        "Default export should not read a local binding omitted by CommonJS recovery.\nOutput:\n{output}"
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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
