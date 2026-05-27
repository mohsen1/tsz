use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

use super::parse_test_source;

#[test]
fn anonymous_default_class_avoids_user_default_1_binding() {
    let source = r#"
const default_1 = "user binding";

export default class {
  value = default_1;
}
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
        output.contains("class default_2"),
        "anonymous default class should avoid colliding with user default_1.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.default = default_2;"),
        "default export should reference the non-colliding synthetic class name.\nOutput:\n{output}"
    );
}

#[test]
fn recovered_anonymous_named_class_export_gets_synthetic_binding() {
    let source = "export class {\n}\n";

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

    let class_pos = output
        .find("class default_")
        .expect("recovered class should get a synthetic default_N binding");
    let binding_start = class_pos + "class ".len();
    let binding_len = output[binding_start..]
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .expect("synthetic binding should be followed by class syntax");
    let binding = &output[binding_start..binding_start + binding_len];
    assert!(
        binding
            .strip_prefix("default_")
            .is_some_and(|suffix| suffix.parse::<u32>().is_ok()),
        "Recovered anonymous exported class should use a default_N binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains(&format!("exports.{binding} = {binding};")),
        "Recovered anonymous exported class should be exported under its synthetic name.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_declare_class_export_does_not_leave_pending_export_name() {
    let source = "export declare class Declared {}\nexport class Live {}\n";

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
        !output.contains("exports.Declared"),
        "declare class exports should be erased without a pending CJS assignment.\nOutput:\n{output}"
    );
    assert!(
        output.contains("class Live"),
        "live class declaration should still be emitted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.Live = Live;"),
        "following live class export should not inherit the erased declare export state.\nOutput:\n{output}"
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

    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
fn plain_class_expression_var_export_uses_split_assignment() {
    let source = "export var simpleExample = class {\n    static getTags() { }\n    tags() { }\n};\nexport var circularReference = class C {\n    static getTags(c) { return c; }\n    tags(c) { return c; }\n};\n";
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
        output.contains("var simpleExample = class {"),
        "Plain exported class expressions should keep a local binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.simpleExample = simpleExample;"),
        "Plain exported class expressions should assign the local binding to exports.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var circularReference = class C {"),
        "Named class expressions should also keep the exported local binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.circularReference = circularReference;"),
        "Named class expression exports should assign after the declaration.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.simpleExample = class"),
        "Plain class expressions should not be emitted as direct exports assignments.\nOutput:\n{output}"
    );
}

#[test]
fn mixed_cjs_export_var_class_expression_keeps_ordered_assignment_schedule() {
    let source = r#"declare function side(label: string): string;
export var a = side("a"), C = class {}, b = side("b");
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

    let local_class = output
        .find("var C = class {")
        .expect("Plain class expression should be emitted as a local binding");
    let export_assignments = output
        .find(r#"exports.a = side("a"), exports.C = C, exports.b = side("b");"#)
        .expect("Mixed export var declarators should share an ordered export assignment statement");

    assert!(
        local_class < export_assignments,
        "Local class binding should be scheduled before the export assignment list.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(r#"var a = side("a"), C = class"#),
        "Inlineable declarators should not be forced into a full local declaration fallback.\nOutput:\n{output}"
    );
}

#[test]
fn transformed_class_expression_var_export_emits_inline_assignment() {
    let source = "export var noPrivates = class {\n    static getTags() { }\n    tags() { }\n    private static ps = -1;\n    private p = 12;\n};\n";
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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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

#[test]
fn es5_esm_class_namespace_merge_uses_bare_iife_after_export_clause() {
    let source = r#"export class C {
}
export namespace C {
export const x = 1;
}
"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
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
        output.contains("export { C };"),
        "ES5 class ESM export should use a separate export clause.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(function (C) {"),
        "Merged namespace should still emit its IIFE.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export (function"),
        "Merged namespace IIFE should not be prefixed with `export`.\nOutput:\n{output}"
    );
}

#[test]
fn es5_esm_erased_namespace_does_not_consume_runtime_export_var() {
    let source = r#"export namespace N {
}
export namespace N {
export const x = 1;
}
"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
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
        output.contains("export var N;"),
        "First runtime namespace block should declare the ESM binding even after an erased namespace.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export (function"),
        "Namespace IIFE should stay bare after the exported var declaration.\nOutput:\n{output}"
    );
}

/// When a class has legacy decorators and is exported in CJS, the
/// `exports.X = X;` pre-assignment should appear exactly once at the class
/// boundary before the decorator assignment.
#[test]
fn decorated_class_export_no_duplicate_exports() {
    let source = "declare var dec: any;\n@dec export class A {}\n";

    let (parser, root) = parse_test_source(source);

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
fn decorated_commonjs_exported_class_static_self_reference_uses_alias() {
    let source = "declare var Something: any;\n@Something({ v: () => Testing123 })\nexport class Testing123 {\n    static prop0: string;\n    static prop1 = Testing123.prop0;\n}\n";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var Testing123_1;"),
        "CommonJS decorated class exports should hoist a self-reference alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("let Testing123 = Testing123_1 = class Testing123"),
        "The exported decorated class should initialize the alias with the class value.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Testing123.prop1 = Testing123_1.prop0;"),
        "Lowered static self-references should use the hoisted alias.\nOutput:\n{output}"
    );
    let export_idx = output
        .find("exports.Testing123 = Testing123;")
        .expect("early CommonJS export assignment should be emitted");
    let static_idx = output
        .find("Testing123.prop1 = Testing123_1.prop0;")
        .expect("static self-reference initializer should be emitted");
    let decorator_idx = output
        .find("exports.Testing123 = Testing123 = Testing123_1 = __decorate([")
        .expect("decorator reassignment should be emitted");
    assert!(
        export_idx < static_idx && static_idx < decorator_idx,
        "The early export assignment must stay between the class value and lowered static initializers.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.Testing123 = Testing123 = Testing123_1 = __decorate(["),
        "The decorator reassignment should preserve the alias in the CommonJS export chain.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_decorated_commonjs_class_namespace_merge_reuses_class_binding() {
    let source = "declare var deco: any;\n@deco\nexport class Widget {}\nexport namespace Widget {\n  export const x = 1;\n}\nWidget.x;\n";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.Widget = Widget;"),
        "CommonJS decorated class export should create the namespace merge binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\nvar Widget;\n"),
        "Merged namespace should reuse the decorated class declaration binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "(function (Widget) {\n    Widget.x = 1;\n})(Widget || (exports.Widget = Widget = {}));"
        ),
        "Merged namespace IIFE should fold the CommonJS export into the existing class binding.\nOutput:\n{output}"
    );
}

#[test]
fn cjs_deferred_enum_export_folds_into_iife_tail() {
    let source = r#"class C {}
enum E {
    A, B
}
export { C, E };
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
        output.contains("})(E || (exports.E = E = {}));"),
        "Deferred enum export should be folded into the IIFE tail.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\nexports.E = E;"),
        "Deferred enum export should not emit a separate assignment.\nOutput:\n{output}"
    );

    let mut amd_printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::AMD,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    amd_printer.set_source_text(source);
    amd_printer.emit(root);
    let amd_output = amd_printer.get_output().to_string();

    assert!(
        amd_output.contains("    var E;"),
        "AMD enum declaration should keep wrapper indentation.\nOutput:\n{amd_output}"
    );
    assert!(
        !amd_output.contains("        var E;"),
        "AMD enum rewrite should not double-indent the enum declaration.\nOutput:\n{amd_output}"
    );
    assert!(
        !amd_output.contains("\n    exports.E = E;"),
        "AMD deferred enum export should not emit a separate assignment.\nOutput:\n{amd_output}"
    );
}

#[test]
fn cjs_deferred_local_export_emits_all_aliases_after_declaration() {
    let source = r#"export { x }
export { x as xx }
export default x;

const x = 'x'
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

    let default_pos = output.find("exports.default = x;").unwrap();
    let declaration_pos = output.find("const x = 'x';").unwrap();
    let export_x_pos = output.find("exports.x = x;").unwrap();
    let export_xx_pos = output.find("exports.xx = x;").unwrap();
    assert!(
        default_pos < declaration_pos,
        "Default export should stay before the declaration.\nOutput:\n{output}"
    );
    assert!(
        declaration_pos < export_x_pos && export_x_pos < export_xx_pos,
        "Named aliases should be deferred until after the declaration.\nOutput:\n{output}"
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
        output.contains("Object.defineProperty(exports, \"__esModule\""),
        "File with import.meta should emit __esModule marker.\nOutput:\n{output}"
    );
}

#[test]
fn node_esm_import_equals_require_uses_create_require() {
    let source = "import mod = require(\"./native.node\");\nmod.doNativeThing(\"good\");\n";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        resolved_node_module_to_esm: true,
        target: ScriptTarget::ES2020,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert_eq!(
        output.trim_end(),
        "import { createRequire as _createRequire } from \"module\";\nconst __require = _createRequire(import.meta.url);\nconst mod = __require(\"./native.node\");\nmod.doNativeThing(\"good\");"
    );
}

#[test]
fn node_esm_import_equals_require_reuses_collision_safe_create_require() {
    let source = "const _createRequire = 1;\nconst __require = 2;\nimport a = require(\"a\");\nimport b = require(\"b\");\na.x;\nb.y;\n";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        resolved_node_module_to_esm: true,
        target: ScriptTarget::ES2020,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import { createRequire as _createRequire_1 } from \"module\";\nconst __require_1 = _createRequire_1(import.meta.url);"),
        "createRequire helper names must avoid source bindings.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const a = __require_1(\"a\");\nconst b = __require_1(\"b\");"),
        "all import-equals declarations should share the synthesized require binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export {};"),
        "runtime import-equals emit should suppress a marker-only export.\nOutput:\n{output}"
    );
}

#[test]
fn node_esm_import_equals_require_preamble_precedes_attached_comment() {
    let source = "// esm format file\nimport fs = require(\"fs\");\nfs.readFile;\n";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        resolved_node_module_to_esm: true,
        target: ScriptTarget::ES2020,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert_eq!(
        output.trim_end(),
        "import { createRequire as _createRequire } from \"module\";\nconst __require = _createRequire(import.meta.url);\n// esm format file\nconst fs = __require(\"fs\");\nfs.readFile;"
    );
}

#[test]
fn node_esm_exported_import_equals_require_uses_export_list() {
    let source = "export import fs2 = require(\"fs\");\n";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        resolved_node_module_to_esm: true,
        target: ScriptTarget::ES2020,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert_eq!(
        output.trim_end(),
        "import { createRequire as _createRequire } from \"module\";\nconst __require = _createRequire(import.meta.url);\nconst fs2 = __require(\"fs\");\nexport { fs2 };"
    );
}

#[test]
fn es_module_declare_export_import_equals_recovers_export_var() {
    let source = r#"namespace x {
    interface c {}
}
declare export import a = x.c;
var b: a;
"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::ES2015,
        target: ScriptTarget::ES2015,
        always_strict: true,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert_eq!(output.trim_end(), "export var a = x.c;\nvar b;");
}

/// A file without any module syntax or import.meta should NOT get __esModule.
#[test]
fn no_import_meta_no_esmodule_marker() {
    let source = r#"const x = 1;
console.log(x);
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
        !output.contains("__esModule"),
        "File without module syntax should NOT get __esModule marker.\nOutput:\n{output}"
    );
}

#[test]
fn system_reexport_setter_uses_bracket_access() {
    let source = r#"export { b } from "./b";
export { default as Foo } from "./b";
"#;

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
        output.contains("var i = I;"),
        "Top-level script import-equals aliases should be preserved even when the target is type-only.\nOutput:\n{output}"
    );
}

/// `import Foo, { bar } from "x"; bar();` - when the default binding is
/// not referenced as a value, tsc elides only the default and keeps the
/// named binding. tsz must match. Regression for #3336.
#[test]
fn esnext_unused_default_beside_used_named_is_elided() {
    let source = r#"import Foo, { bar } from "./dep";
bar();
export {};
"#;

    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import { bar } from \"./dep\""),
        "Used named binding must be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Foo"),
        "Unused default binding should be elided.\nOutput:\n{output}"
    );
}

/// Same import shape but with the default actually used as a value;
/// the default must be preserved beside the named binding.
#[test]
fn esnext_used_default_beside_used_named_is_preserved() {
    let source = r#"import Foo, { bar } from "./dep";
bar();
new Foo();
export {};
"#;

    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import Foo, { bar } from \"./dep\""),
        "Both default and named bindings must be preserved when both are used.\nOutput:\n{output}"
    );
}

/// `import Foo, * as ns from "x"; ns.bar();` - unused default beside a
/// used namespace binding must drop only the default.
#[test]
fn esnext_unused_default_beside_used_namespace_is_elided() {
    let source = r#"import Foo, * as ns from "./dep";
ns.bar();
export {};
"#;

    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import * as ns from \"./dep\""),
        "Used namespace binding must be preserved without the unused default.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Foo"),
        "Unused default binding should be elided.\nOutput:\n{output}"
    );
}

/// Bound-name choice must not matter - same elision rule for any
/// default identifier name.
#[test]
fn esnext_unused_default_elision_is_name_agnostic() {
    let source = r#"import X, { y } from "./dep";
y();
export {};
"#;

    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import { y } from \"./dep\""),
        "Used named binding must be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(" X "),
        "Unused default binding should be elided regardless of its identifier name.\nOutput:\n{output}"
    );
}

/// verbatimModuleSyntax must keep the source clause exactly - no elision.
#[test]
fn esnext_verbatim_module_syntax_keeps_unused_default() {
    let source = r#"import Foo, { bar } from "./dep";
bar();
export {};
"#;

    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::ESNext,
        verbatim_module_syntax: true,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import Foo, { bar } from \"./dep\""),
        "verbatimModuleSyntax must preserve the original import clause.\nOutput:\n{output}"
    );
}
