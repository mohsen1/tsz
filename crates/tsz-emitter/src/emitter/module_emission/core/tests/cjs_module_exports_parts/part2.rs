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
