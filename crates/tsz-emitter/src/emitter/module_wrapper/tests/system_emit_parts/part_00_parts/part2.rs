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
