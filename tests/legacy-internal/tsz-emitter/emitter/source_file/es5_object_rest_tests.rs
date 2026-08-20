use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use crate::output::printer::{PrintOptions, Printer};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn esm_emits_export_empty_when_only_type_exports() {
    // When a file's only module syntax is `export {};`, it should be preserved
    // to maintain ESM semantics.
    let source = "export {};\nconst x = 1;\n";
    let (parser, root) = parse_test_source(source);
    let mut printer = Printer::new(
        &parser.arena,
        PrintOptions {
            module: crate::emitter::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("export {};"),
        "Sole `export {{}}` should be preserved for ESM semantics.\nOutput:\n{output}"
    );
}

#[test]
fn esm_top_level_using_real_export_suppresses_export_empty() {
    let source =
        "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nexport class C {}\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::ESNext,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert_eq!(
        output.matches("export {};").count(),
        0,
        "A real export inside a top-level using scope should suppress the deferred empty export marker.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export { C };"),
        "The hoisted ESM export for the class should still be emitted.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_assignment_marks_rest_helper() {
    let source = "let bar: {};\n({ ...bar } = {});\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __rest = "),
        "Object-rest assignment should request the __rest helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(bar = __rest({}, []));"),
        "Object-rest assignment lowering should still call __rest.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_assignment_literal_rest_targets_use_temps() {
    let source = "let a: any;\n({...{}} = {});\n({...({})} = {});\n({...[]} = {});\n({...([])} = {});\n({...{ a }} = { a: 1 });\n({...({ a })} = { a: 1 });\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a, _b;"),
        "Bare literal rest targets should reserve hoisted assignment temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_a = __rest({}, []));"),
        "Bare object literal rest target should lower to a temp assignment.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(({}) = __rest({}, []));"),
        "Parenthesized object literal rest target should stay as a destructuring target.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_b = __rest({}, []));"),
        "Bare array literal rest target should lower to a temp assignment.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(([]) = __rest({}, []));"),
        "Parenthesized array literal rest target should stay as a destructuring target.\nOutput:\n{output}"
    );
    assert!(
        output.contains("({ a } = __rest({ a: 1 }, []));"),
        "Non-empty object literal rest target should stay as a destructuring target.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(({ a }) = __rest({ a: 1 }, []));"),
        "Parenthesized non-empty object literal rest target should also stay as a destructuring target.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_assignment_value_position_returns_rhs_value() {
    let source = "let bar: any;\nlet value = ({ ...bar } = { x: 1 });\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a;"),
        "Value-position object-rest assignment should hoist an RHS value temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = { x: 1 }, bar = __rest(_a, []), _a"),
        "Object-rest assignment expressions must evaluate to their RHS value.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("let value = bar = __rest"),
        "Value-position object-rest assignment must not evaluate to the rest target.\nOutput:\n{output}"
    );
}

#[test]
fn dynamic_object_rest_keeps_simple_binding_groups() {
    let source = "let obj = {};\nlet prop: any, other: any, props: any;\nlet { prop = { ...obj }, ['k' + '']: other = { ...obj }, ...props } = {};\n({ prop = { ...obj }, ['k' + '']: other = { ...obj }, ...props } = {});\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES2017,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert_eq!(
        output
            .matches("{ prop = Object.assign({}, obj) } =")
            .count(),
        2,
        "Simple binding groups should stay as object patterns before dynamic keys.\nOutput:\n{output}"
    );
    assert!(
        output.contains("other = ") && output.contains("__rest("),
        "Dynamic computed keys should still lower through temps and feed __rest.\nOutput:\n{output}"
    );
}

#[test]
fn nested_object_rest_assignment_inlines_single_array_source() {
    let source = "var x: any;\n[{ ...x }] = [{ abc: 1 }];\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES2017,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("[_a] = [{ abc: 1 }], x = __rest(_a, []);"),
        "Single-element array assignment with nested object-rest should inline the RHS.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a = [{ abc: 1 }]"),
        "The RHS should not be copied to a separate source temp first.\nOutput:\n{output}"
    );
}

#[test]
fn defaulted_nested_object_rest_assignment_uses_resolved_source() {
    let source = "let a: any, b: any, c: any = { x: { a: 1, y: 2 } }, d: any;\n({ x: { a, ...b } = d } = c);\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a, _b;"),
        "Defaulted nested object-rest assignment should hoist both evaluation temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "(_a = c.x, _b = _a === void 0 ? d : _a, { a } = _b, b = __rest(_b, [\"a\"]));"
        ),
        "Nested object rest must use the resolved default source, not the default expression.\nOutput:\n{output}"
    );
}

#[test]
fn es5_defaulted_nested_object_rest_assignment_uses_resolved_source() {
    let source = "let a: any, b: any, c: any = { x: { a: 1, y: 2 } }, d: any;\n({ x: { a, ...b } = d } = c);\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a, _b;"),
        "ES5 nested object-rest assignment should hoist both evaluation temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "(_a = c.x, _b = _a === void 0 ? d : _a, a = _b.a, b = __rest(_b, [\"a\"]));"
        ),
        "ES5 nested object rest must use the resolved default source, not the default expression.\nOutput:\n{output}"
    );
}

#[test]
fn es5_block_scoped_destructuring_uses_renamed_binding_targets() {
    let source = "var z0: any, z1: any, z2: any;\n{\n    let [z0] = [1];\n    use(z0);\n    let { a: z1 } = { a: 1 };\n    use(z1);\n    let [...z2] = [1, 2];\n    use(z2);\n}\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var z0_1 = [1][0];"),
        "Array binding declaration target should use the block-scoped emitted name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("use(z0_1);"),
        "Array binding references should match the declaration target.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var z1_1 = { a: 1 }.a;"),
        "Object binding declaration target should use the block-scoped emitted name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("use(z1_1);"),
        "Object binding references should match the declaration target.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var z2_1 = [1, 2].slice(0);"),
        "Array rest binding declaration target should use the block-scoped emitted name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("use(z2_1);"),
        "Array rest references should match the declaration target.\nOutput:\n{output}"
    );
}

#[test]
fn es5_block_scoped_object_literal_keys_keep_source_names() {
    let source = "var x0: any;\nif (true) {\n    let x0;\n    var obj1 = { x0: x0 };\n    var obj2 = { x0 };\n}\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var x0_1;"),
        "Renamed block-scoped declaration should emit without a synthetic initializer.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var obj1 = { x0: x0_1 };"),
        "Explicit object-literal keys should keep source names while values use renamed bindings.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var obj2 = { x0: x0_1 };"),
        "Shorthand object-literal keys should expand with the source key and renamed value.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("x0_1: x0_1"),
        "Object-literal keys must not be block-scope renamed.\nOutput:\n{output}"
    );
}

#[test]
fn es5_block_scoped_object_destructuring_keys_keep_source_names() {
    let source = "var x: any, y: any, z: any;\nif (true) {\n    let { x: x } = { x: 0 };\n    let { y } = { y: 0 };\n    let z;\n    ({ z: z } = { z: 0 });\n    ({ z } = { z: 0 });\n}\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var x_1 = { x: 0 }.x;"),
        "Explicit object binding should read the source key and write the renamed target.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var y_1 = { y: 0 }.y;"),
        "Shorthand object binding should read the source key and write the renamed target.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var z_1;"),
        "Renamed block-scoped declaration should emit without a synthetic initializer.\nOutput:\n{output}"
    );
    assert!(
        output.matches("(z_1 = { z: 0 }.z);").count() == 2,
        "Both explicit and shorthand destructuring assignments should write the renamed target.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ z_1: 0 }"),
        "Destructuring source object-literal keys must not be block-scope renamed.\nOutput:\n{output}"
    );
}

#[test]
fn es5_class_method_nested_block_keeps_outer_names_nameable() {
    let source = "declare function use(a: any): void;\nvar shadowed: any;\nclass C {\n    m() {\n        {\n            let shadowed = 1;\n            use(shadowed);\n        }\n        use(shadowed);\n    }\n}\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var shadowed_1 = 1;"),
        "Nested method block declaration should not capture the outer name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("use(shadowed_1);"),
        "Nested method block references should follow the renamed binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("        use(shadowed);\n"),
        "Method-scope reference after the nested block should still name the outer binding.\nOutput:\n{output}"
    );
}

#[test]
fn es5_namespace_blocks_share_block_scope_rename_state() {
    let source = "declare function use(value: any): void;\nvar x: any;\nvar y: any;\nvar z: any;\nfunction first() {\n    {\n        let x = 1;\n        let [y] = [1];\n        let { a: z } = { a: 1 };\n    }\n}\nnamespace N {\n    {\n        let x = 2;\n        use(x);\n        let [y] = [2];\n        use(y);\n        let { a: z } = { a: 2 };\n        use(z);\n    }\n    use(x);\n    use(y);\n    use(z);\n}\nnamespace Local {\n    let [y] = [1];\n    use(y);\n}\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var x_1 = 1;"),
        "The first nested block should reserve the first suffix for `x`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var x_2 = 2;") && output.contains("use(x_2);"),
        "Namespace nested blocks should inherit prior suffix reservations and rewrite references.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var y_2 = [2][0];") && output.contains("use(y_2);"),
        "Namespace array binding declarations should use the inherited suffix sequence.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var z_2 = { a: 2 }.a;") && output.contains("use(z_2);"),
        "Namespace object binding declarations should use the inherited suffix sequence.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var y = [1][0];\n    use(y);") && !output.contains("use(Local.y);"),
        "Function-level namespace destructuring locals should shadow namespace exports without qualification.\nOutput:\n{output}"
    );
}

#[test]
fn es5_single_leaf_nested_destructuring_inlines_access_path() {
    let source = "var z1: any, z3: any;\n{\n    const [{ a: z1 }] = [{ a: 1 }];\n    use(z1);\n    const { a: { b: z3 } } = { a: { b: 1 } };\n    use(z3);\n}\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        always_strict: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var z1_1 = [{ a: 1 }][0].a;"),
        "Single-leaf array/object destructuring should inline the access path.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var z3_1 = { a: { b: 1 } }.a.b;"),
        "Single-leaf nested object destructuring should inline the access path.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _a = [{ a: 1 }]") && !output.contains("var _b = _a[0]"),
        "Single-leaf nested destructuring should not allocate avoidable temps.\nOutput:\n{output}"
    );
}

#[test]
fn esm_exported_object_rest_keeps_temp_local() {
    let source = "export const { x, ...rest } = { x: 'x', y: 'y' }, y = 3;\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        no_emit_helpers: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let plan = LoweringPass::new(&parser.arena, &ctx).run_plan(root);
    let mut printer = EmitterPrinter::with_emit_plan_and_options(&parser.arena, plan, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a;\nexport var x = (_a = { x: 'x', y: 'y' }, _a).x, rest = __rest(_a, [\"x\"]), y = 3;"),
        "Exported object-rest temp should be hoisted outside the export list.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export var _a"),
        "The synthesized temp must not become an exported binding.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_block_preserves_recovered_module_syntax() {
    let source = "namespace P {\n    {\n        namespace M { }\n        export = M;\n        function foo() { }\n        export { foo };\n        import I = M;\n        import I2 = require(\"foo\");\n        import * as Foo from \"ambient\";\n        import bar from \"ambient\";\n        import { baz } from \"ambient\";\n    }\n}\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("export = M;"),
        "Recovered export assignment should be preserved inside namespace block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export { foo };"),
        "Recovered local export should be preserved inside namespace block.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var I = M;"),
        "Recovered namespace alias import should still be erased.\nOutput:\n{output}"
    );
    assert!(
        output.contains("import I2 = require(\"foo\");"),
        "Recovered import-equals should be preserved inside namespace block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("import * as Foo from \"ambient\";"),
        "Recovered namespace import should be preserved inside namespace block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("import bar from \"ambient\";"),
        "Recovered default import should be preserved inside namespace block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("import { baz } from \"ambient\";"),
        "Recovered named import should be preserved inside namespace block.\nOutput:\n{output}"
    );
}

#[test]
fn amd_es5_reexported_enum_folds_export_into_iife() {
    let source = "enum E { A }\nexport { E };\n";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        module: ModuleKind::AMD,
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("})(E || (exports.E = E = {}));"),
        "AMD ES5 enum re-export should fold the export into the enum IIFE.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\n    exports.E = E;"),
        "AMD ES5 enum re-export should not emit a separate export assignment.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_declare_module_recovers_runtime_class_shell() {
    let source = "declare module {\n    export class XDate {\n        getDay(): number;\n        static now(): number;\n    }\n}\nvar d = new XDate();\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("declare;\nmodule;\n{"),
        "Anonymous declare module should recover declare/module shell.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export class XDate"),
        "Runtime class shell should be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("getDay"),
        "Ambient class members should remain erased.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var d = new XDate();"),
        "Following runtime statement should still emit.\nOutput:\n{output}"
    );
}
