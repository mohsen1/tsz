use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use crate::output::printer::{PrintOptions, Printer};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn named_tc39_decorated_class_expression_skips_set_function_name() {
    let source = "declare var dec: any;\nexport const C = @dec class C {};\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        import_helpers: true,
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
        output.contains("__esDecorate"),
        "Class decorator transform should still run.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__setFunctionName"),
        "Named class expressions should not emit named-evaluation helper calls.\nOutput:\n{output}"
    );
}

#[test]
fn default_tc39_decorated_named_class_keeps_class_1_binding() {
    let source = "declare var dec: any;\nexport default @dec class class_1 {};\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2020,
        import_helpers: true,
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
        output.contains("var class_1 = _classThis = class"),
        "Named default decorated class should preserve the class_1 runtime binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_classThis, \"class_1\")"),
        "Named default decorated class should use its source name for setFunctionName.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var default_1 = _classThis = class class_1"),
        "Default export rewriting must not rename a real source class_1 binding.\nOutput:\n{output}"
    );
}

#[test]
fn ambient_class_parenthesized_tail_emits_recovered_expression() {
    let source = "declare class foo();\nfunction foo() {}\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            always_strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("\"use strict\";\n();\nfunction foo() { }"),
        "Malformed ambient class tail should emit the recovered `();` expression.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_var_class_keyword_emits_recovered_class_tail() {
    let source = "var export;\nvar foo;\nvar class;\nvar bar;\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            always_strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var ;\nclass {\n}\n;\nvar bar;"),
        "`var class;` should emit tsc's recovered anonymous class tail.\nOutput:\n{output}"
    );
}

#[test]
fn class_mapped_type_member_emits_recovered_tail() {
    let source = "type PlaceType = 'openSky' | 'roofed' | 'garage';\nclass C {\n    [P in PlaceType]: any\n}\nconst D = class {\n    [P in PlaceType]: any\n};\nconst E = class {\n    [P in 'a' | 'b']: any\n};\n";

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
        output.contains("var _a, _b;"),
        "Class expressions with recovered mapped members should reserve comma temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("class C {\n}\nP in PlaceType;"),
        "Class declarations should emit recovered mapped-member tails after the class.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const D = (_a = class {\n    },\n    P in PlaceType,\n    _a);"),
        "Class expression mapped-member tail should be a comma item before the temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const E = (_b = class {\n    },\n    P in 'a' | 'b',\n    _b);"),
        "String-literal mapped clauses should preserve the recovered tail text.\nOutput:\n{output}"
    );
}

#[test]
fn reserved_enum_name_emits_anonymous_enum_and_reserved_statement() {
    for (source, recovered_statement) in [
        ("enum void {}", "void {};"),
        ("enum typeof {}", "typeof {};"),
        ("enum delete {}", "delete {};"),
        ("enum class {}", "class {\n}"),
        ("enum true {}", "true;\n{ }"),
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert_eq!(
            output.trim_end(),
            format!(
                "\"use strict\";\nvar ;\n(function () {{\n}})( || ( = {{}}));\n{recovered_statement}"
            ),
            "{source}: reserved enum recovery should preserve tsc-compatible emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn reserved_variable_name_emits_empty_decl_keyword_and_initializer_statements() {
    for (source, expected_tail) in [
        ("var typeof = 10;", "var ;\ntypeof ;\n10;"),
        ("var void = value;", "var ;\nvoid ;\nvalue;"),
        ("var delete = target;", "var ;\ndelete ;\ntarget;"),
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert_eq!(
            output.trim_end(),
            format!("\"use strict\";\n{expected_tail}"),
            "{source}: reserved variable-name recovery should preserve tsc-compatible emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn reserved_import_equals_name_emits_recovered_require_and_keyword_loop() {
    for (source, expected_tail) in [
        (
            "import while = require(\"dfdf\");",
            "require();\nwhile ( = require(\"dfdf\"))\n    ;",
        ),
        (
            "import for = require(\"dfdf\");",
            "require();\nfor ( = require(\"dfdf\"))\n    ;",
        ),
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                module: ModuleKind::CommonJS,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert_eq!(
            output.trim_end(),
            format!(
                "\"use strict\";\nObject.defineProperty(exports, \"__esModule\", {{ value: true }});\n{expected_tail}"
            ),
            "{source}: reserved import-equals recovery should preserve tsc-compatible emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn reserved_array_binding_name_emits_recovered_keyword_statements() {
    for (source, expected_tail) in [
        (
            "var [debugger, if] = [1, 2];",
            "var [];\ndebugger;\nif ()\n    ;\n[1, 2];",
        ),
        (
            "var [debugger, while] = value;",
            "var [];\ndebugger;\nwhile ()\n    ;\nvalue;",
        ),
        ("var [debugger] = value;", "var [];\ndebugger;\nvalue;"),
        (
            "var [debugger,\n if] = value;",
            "var [];\ndebugger;\nif ()\n    ;\nvalue;",
        ),
        (
            "var [debugger, if, while] = value;",
            "var [];\ndebugger;\nif (, )\n    while ()\n        ;\nvalue;",
        ),
        (
            "var [debugger, if] = [1, 2];\nenum void {}",
            "var [];\ndebugger;\nif ()\n    ;\n[1, 2];\n(function () {\n})( || ( = {}));\nvoid {};",
        ),
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert_eq!(
            output.trim_end(),
            format!("\"use strict\";\n{expected_tail}"),
            "{source}: reserved array-binding recovery should preserve tsc-compatible emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn array_binding_with_reserved_default_values_emits_normally() {
    // Reserved words that appear as default values (`= true`) or inside the
    // right-hand initializer (`= [.., null]`) are values, not binding names,
    // so the reserved-array-binding error recovery must not fire.
    for source in [
        "var [a = true, b = false] = [1, 2];",
        "var [c, d] = [undefined, null];",
        "var [e = [null]] = [[1]];",
        "var [f, g] = [1, 2, \"string\", true];",
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            !output.contains("var [];"),
            "{source}: valid array binding must not trigger reserved-name recovery.\nOutput:\n{output}"
        );
        assert!(
            output.contains('['),
            "{source}: array binding pattern should still emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn identifier_beginning_with_keyword_emits_normally() {
    // Ordinary identifiers that merely start with a reserved keyword
    // (`var1`, `function1`, `typeofx`) must not be mistaken for reserved
    // binding names by the variable-declaration error recovery.
    for (source, expected_name) in [
        ("var var1 = 0;", "var1"),
        ("var function1 = 1;", "function1"),
        ("var typeofx = 2;", "typeofx"),
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            !output.contains("var ;"),
            "{source}: identifier starting with a keyword must not trigger recovery.\nOutput:\n{output}"
        );
        assert!(
            output.contains(expected_name),
            "{source}: declaration name should still emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn reserved_function_name_emits_anonymous_function_and_keyword_arrow_tail() {
    for (source, expected_tail) in [
        ("function throw() {}", "function () { }\nthrow () => { };"),
        (
            "function return(value) {}",
            "function (value) { }\nreturn () => { };",
        ),
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert_eq!(
            output.trim_end(),
            format!("\"use strict\";\n{expected_tail}"),
            "{source}: reserved function-name recovery should preserve tsc-compatible emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn reserved_namespace_name_emits_keyword_and_recovered_body_statement() {
    for (source, expected_tail) in [
        ("namespace void {}", "namespace;\nvoid {};"),
        ("namespace typeof {}", "namespace;\ntypeof {};"),
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert_eq!(
            output.trim_end(),
            format!("\"use strict\";\n{expected_tail}"),
            "{source}: reserved namespace-name recovery should preserve tsc-compatible emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn reserved_parameter_names_recover_without_semantic_type_proxy() {
    for (source, expected_tail) in [
        ("function f(default: number) {}", "function f() { }"),
        (
            "class C { m(null: string) {} }",
            "class C {\n    m(, string) { }\n}",
        ),
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert_eq!(
            output.trim_end(),
            format!("\"use strict\";\n{expected_tail}"),
            "{source}: reserved parameter recovery should preserve tsc-compatible emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn this_parameter_default_new_initializer_emits_recovered_tail() {
    for (source, expected_tail) in [
        (
            "function f(this: C = new C()): number { return this.n; }",
            "();\nnumber;\n{\n    return this.n;\n}",
        ),
        (
            "function renamed(this: Widget = new Widget()): Result { return this.value; }",
            "();\nResult;\n{\n    return this.value;\n}",
        ),
        (
            "function normal(this: Widget): Result { return this.value; }",
            "function normal() { return this.value; }",
        ),
    ] {
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                always_strict: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert_eq!(
            output.trim_end(),
            format!("\"use strict\";\n{expected_tail}"),
            "{source}: defaulted `this` parameter recovery should preserve tsc-compatible emit.\nOutput:\n{output}"
        );
    }
}

#[test]
fn unmatched_decorator_type_assertion_emits_empty_statement() {
    let source = "@<[[import(obju2c77,\n";

    let mut parser = ParserState::new(
        "parseUnmatchedTypeAssertion.ts".to_string(),
        source.to_string(),
    );
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            always_strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert_eq!(
        output.trim_end(),
        "\"use strict\";\n;",
        "Malformed decorator type assertion should preserve tsc's recovered empty statement.\nOutput:\n{output}"
    );
}

#[test]
fn recovered_comma_separated_overload_signatures_emit_empty_bodies() {
    let source = "function f1(), function f1();\nfunction f2(), function f2() {}\nfunction f3() {}, function f3();\n\nclass C {\n    m1(), m1();\n    m2(), m2() {}\n    m3() {}, m3();\n}\n";

    let mut parser = ParserState::new("overloadConsecutiveness.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            always_strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert_eq!(
        output.trim_end(),
        "\"use strict\";\nfunction f1() { }\nfunction f2() { }\nfunction f2() { }\nfunction f3() { }\nclass C {\n    m1() { }\n    m2() { }\n    m2() { }\n    m3() { }\n}",
        "Recovered comma-separated overload declarations should emit tsc-aligned empty bodies.\nOutput:\n{output}"
    );
}

#[test]
fn recovered_class_member_enum_emits_after_class() {
    let source = "namespace M {\n    class C {\n\n    enum E {\n    }\n}\n";

    let mut parser = ParserState::new(
        "parserErrorRecovery_ClassElement2.ts".to_string(),
        source.to_string(),
    );
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            always_strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains(
            "    class C {\n    }\n    let E;\n    (function (E) {\n    })(E || (E = {}));"
        ),
        "Recovered enum class member should emit as a sibling after the class.\nOutput:\n{output}"
    );
}

#[test]
fn recovered_nested_class_emits_after_class() {
    let source = "class C {\n\n// Classes can't be nested.  So we should bail out of parsing here and recover\n// this as a source unit element.\nclass D {\n}";

    let mut parser = ParserState::new(
        "parserErrorRecovery_ClassElement1.ts".to_string(),
        source.to_string(),
    );
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            always_strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("class D {\n}"),
        "Recovered nested class should emit as a sibling after the outer class.\nOutput:\n{output}"
    );
}

#[test]
fn esm_suppresses_redundant_export_empty_when_real_exports_exist() {
    // When a file has both `export {};` and `export { C };`, the empty export
    // is redundant and should be suppressed. tsc omits it.
    let source = "export {};\nclass C {}\nexport { C };\n";
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

    // Should NOT contain `export {};` since `export { C };` is present
    let export_empty_count = output.matches("export {};").count();
    assert_eq!(
        export_empty_count, 0,
        "Redundant `export {{}}` should be suppressed when real exports exist.\nOutput:\n{output}"
    );
    assert!(
        output.contains("export { C }"),
        "Real export should be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn system_register_bundle_suppresses_top_level_use_strict() {
    // In --outFile bundles with --module system, tsc does NOT emit "use strict"
    // before System.register() calls. Each callback has its own "use strict" inside.
    let source = r#"System.register("a", [], function (exports_1, context_1) {
"use strict";
var A;
var __moduleName = context_1 && context_1.id;
return {
    setters: [],
    execute: function () {
        A = class A { };
        exports_1("A", A);
    }
};
});
"#;
    let mut parser = ParserState::new("bundle.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrinterOptions {
        module: ModuleKind::System,
        always_strict: true,
        ..Default::default()
    };
    let mut printer = EmitterPrinter::with_options(&parser.arena, opts);
    printer.set_current_root_js_source(true);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    // "use strict" should NOT appear before System.register
    let system_pos = output
        .find("System.register")
        .expect("System.register should be emitted");
    let use_strict_before = output[..system_pos].contains("\"use strict\"");
    assert!(
        !use_strict_before,
        "\"use strict\" should NOT appear before System.register() in bundled output.\nOutput:\n{output}"
    );
}

#[test]
fn js_passthrough_gets_use_strict_from_always_strict() {
    // tsc adds "use strict" to .js passthrough files when alwaysStrict is enabled,
    // just like for .ts files. The alwaysStrict option is not TS-only.
    let source = "const x = 0;\n";
    let mut parser = ParserState::new("sub.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrinterOptions {
        module: ModuleKind::CommonJS,
        always_strict: true,
        ..Default::default()
    };
    let mut printer = EmitterPrinter::with_options(&parser.arena, opts);
    printer.set_current_root_js_source(true);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.starts_with("\"use strict\";"),
        "JS passthrough files should get \"use strict\" from alwaysStrict.\nOutput:\n{output}"
    );
}

#[test]
fn js_passthrough_esm_no_use_strict_from_always_strict() {
    // ESM JS files should NOT get "use strict" because ESM is implicitly strict.
    // The !(is_es_module_output && is_file_module) guard handles this.
    let source = "export const x = 0;\n";
    let mut parser = ParserState::new("sub.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrinterOptions {
        module: ModuleKind::ESNext,
        always_strict: true,
        ..Default::default()
    };
    let mut printer = EmitterPrinter::with_options(&parser.arena, opts);
    printer.set_current_root_js_source(true);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("\"use strict\""),
        "ESM JS files should NOT get \"use strict\" (ESM is implicitly strict).\nOutput:\n{output}"
    );
}

#[test]
fn root_js_recovery_preserves_invalid_declaration_modifiers() {
    let source = "\
class C {
    async constructor() { }
    async field = 1
    set invariant() { }
}
async export function f() { }
async async function g() { }
function params(static x, export y, async z) { }
async const value = 1
async import 'assert'
async export { f }
export import 'fs'
export export { g }
export export var duplicateExport = 1
export static var staticExport = 1
function outer() {
    static function inner() { }
}
const object = {
    static method() { }
    [console.log('oh no'), 2]: 'hi',
    #secret: 1,
    export cantExportProperties: 4,
}
const { ...rest = true } = object
const tri = import('1','2','3')
";
    let mut parser = ParserState::new("plain.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrinterOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ESNext,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    let mut printer = EmitterPrinter::with_options(&parser.arena, opts);
    printer.set_current_root_js_source(true);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    for expected in [
        "constructor()",
        "async field = 1;",
        "set invariant() { }",
        "async export function f() { }",
        "async async function g() { }",
        "function params(static x, export y, async z) { }",
        "async const value = 1;",
        "async import 'assert';",
        "export { f };",
        "export import 'fs';",
        "export { g };",
        "export export var duplicateExport = 1;",
        "export static var staticExport = 1;",
        "static function inner() { }",
        "static method() { }",
        "[console.log('oh no'), 2]: 'hi'",
        "#secret: 1",
        "cantExportProperties: 4",
        "const { ...rest = true } = object;",
        "const tri = import('1','2','3');",
    ] {
        assert!(
            output.contains(expected),
            "Expected recovered JS modifier output `{expected}`.\nOutput:\n{output}"
        );
    }
    assert!(
        !output.contains("async;\n"),
        "Recovered modifiers should stay attached to declarations.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("async export {"),
        "Stray async before a named export should not be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var iant"),
        "Recovered setter name tail should not emit as a synthetic variable statement.\nOutput:\n{output}"
    );
}
