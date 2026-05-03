use crate::emitter::{ModuleKind, Printer, PrinterOptions};
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
