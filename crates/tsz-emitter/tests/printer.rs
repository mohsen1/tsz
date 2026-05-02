use super::*;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;

/// Parse, lower, and print a source string with the given options.
///
/// Convenience wrapper for tests that don't need access to the parser
/// arena. Uses `"test.ts"` as the file name and returns the printed code.
fn parse_lower_print(source: &str, opts: PrintOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn test_print_options() {
    let opts = PrintOptions::es5();
    assert!(matches!(opts.target, ScriptTarget::ES5));

    let opts = PrintOptions::commonjs();
    assert!(matches!(opts.module, ModuleKind::CommonJS));

    let opts = PrintOptions::es5_commonjs();
    assert!(matches!(opts.target, ScriptTarget::ES5));
    assert!(matches!(opts.module, ModuleKind::CommonJS));
}

#[test]
fn test_streaming_writer() {
    let mut output = Vec::new();
    {
        let mut printer = StreamingPrinter::new(&mut output);
        printer
            .write("hello")
            .expect("writing to Vec<u8> should not fail");
        printer
            .write(" ")
            .expect("writing to Vec<u8> should not fail");
        printer
            .write("world")
            .expect("writing to Vec<u8> should not fail");
        printer
            .flush()
            .expect("flushing to Vec<u8> should not fail");
    }
    assert_eq!(
        String::from_utf8(output).expect("output should be valid UTF-8"),
        "hello world"
    );
}

#[test]
fn arrow_default_nullish_temp_is_scoped_to_es2015_body() {
    let source = "const a = (): string | undefined => undefined;\n((b = a() ?? \"d\") => {})();";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("const a = () => undefined;"),
        "Type annotations should still be erased from the arrow initializer.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "((b) => {\n    var _a;\n    if (b === void 0) { b = (_a = a()) !== null && _a !== void 0 ? _a : \"d\"; }\n})();"
        ),
        "Default initializer temp should be declared inside the generated arrow body.\nOutput:\n{output}"
    );
    assert!(
        !output.starts_with("var _a;"),
        "Default initializer temp must not leak to file scope.\nOutput:\n{output}"
    );
}

#[test]
fn arrow_default_optional_chain_temp_is_scoped_to_es5_body() {
    let source = "const a = (): { d: string } | undefined => undefined;\n((b = a()?.d) => {})();";
    let output = parse_lower_print(source, PrintOptions::es5());

    assert!(
        output.contains(
            "(function (b) {\n    var _a;\n    if (b === void 0) { b = (_a = a()) === null || _a === void 0 ? void 0 : _a.d; }\n})();"
        ),
        "Default initializer optional-chain temp should be declared inside the ES5 function body.\nOutput:\n{output}"
    );
}

#[test]
fn decorated_anonymous_class_expression_sets_empty_function_name() {
    let source = "declare let dec: any;\n(@dec class {});";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2022,
            ..Default::default()
        },
    );

    assert!(
        output.contains("static { __setFunctionName(_classThis, \"\"); }"),
        "Anonymous decorated class expressions should set an empty function name.\nOutput:\n{output}"
    );
}

#[test]
fn test_es6_generator_param_named_yield_keeps_identifier_text() {
    let source = "function* foo(a = yield, yield) {}";
    let output = parse_lower_print(source, PrintOptions::es6());
    assert_eq!(output, "function* foo(a = yield, yield) { }\n");
}

#[test]
fn test_optional_catch_binding_downlevel_to_param() {
    let source = "try {\n} catch {\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2018,
            ..Default::default()
        },
    );
    assert!(
        output.contains("catch (_a)"),
        "Expected catch (_a) in downleveled output, got: {output}"
    );

    let output_es2020 = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );
    assert!(
        !output_es2020.contains("catch (_a)"),
        "ES2020+ should preserve optional catch binding"
    );
}

#[test]
fn test_optional_catch_binding_multiple_get_unique_names() {
    let source = "try {} catch {}\ntry {} catch {}\ntry {} catch {}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2018,
            ..Default::default()
        },
    );
    assert!(
        output.contains("catch (_a)"),
        "First catch should use _a, got: {output}"
    );
    assert!(
        output.contains("catch (_b)"),
        "Second catch should use _b, got: {output}"
    );
    assert!(
        output.contains("catch (_c)"),
        "Third catch should use _c, got: {output}"
    );
}

#[test]
fn test_exponentiation_downlevel_to_math_pow() {
    let source = "const x = 2 ** 3;\nlet y = 2;\ny **= 3;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(output.contains("Math.pow(2, 3)"));
    assert!(output.contains("y = Math.pow(y, 3)"));
}

#[test]
fn test_optional_call_downlevel_to_conditional() {
    let source = "const fn = () => 1;\nconst obj = { m() { return this; } };\nfn?.();\nobj?.m();\nobj.m?.();\nobj?.m?.();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2019,
            ..Default::default()
        },
    )
    .code;
    assert!(output.contains("fn === null || fn === void 0 ? void 0 : fn()"));
    assert!(output.matches(".call(").count() >= 2);
    assert!(!output.contains("?.("));
}

#[test]
fn test_optional_call_es2020_syntax_preserved() {
    let source = "const fn = () => 1;\nconst obj = { m() { return this; } };\nfn?.();\nobj?.m();\nobj.m?.();\nobj?.m?.();\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );
    assert!(output.contains("fn?.()"));
    assert!(output.contains("obj?.m()"));
    assert!(output.contains("obj.m?.()"));
    assert!(output.contains("obj?.m?.()"));
    assert!(!output.contains("void 0"));
}

#[test]
fn test_optional_call_spread_downlevel_es5() {
    let source = "const fn = function (...args) { return args; };\nconst obj = { m(...args) { return args; } };\nfn?.(...[1], 2);\nobj?.m(...[1], 2);\nobj.m?.(...[1], 2);\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    assert!(output.contains(".__spreadArray"));
    assert!(output.contains(".apply(void 0,"));
    assert!(output.contains(".call.apply"));
    assert!(!output.contains("?.("));
}

#[test]
fn test_commonjs_empty_named_import_emits_bare_require() {
    let source = "import {} from \"./side\";\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(output.contains("require(\"./side\");"));
    assert!(!output.contains("var side_1 = require(\"./side\");"));
}

#[test]
fn test_commonjs_type_only_named_import_is_elided() {
    let source = "import { type Foo } from \"./types\";\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(!output.contains("require(\"./types\")"));
}

#[test]
fn test_commonjs_module_temp_vars_do_not_collide() {
    let source = "import { x } from \"./foo\";\nexport { y } from \"../foo\";\nconsole.log(x);\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("foo_1 = require(\"./foo\");"),
        "expected foo_1, got:\n{output}"
    );
    assert!(
        output.contains("foo_2 = require(\"../foo\");"),
        "expected foo_2, got:\n{output}"
    );
}

#[test]
fn test_es5_class_expression_uses_variable_declaration_name() {
    let source = "const C = class { method() { return 1; } };";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var C = /** @class */"),
        "Expected class expression to use surrounding variable name.\nOutput: {output}"
    );
}

#[test]
fn test_es5_class_expression_uses_assignment_lhs_name() {
    let source = "let C;\nC = class { method() { return 1; } };";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var C = /** @class */"),
        "Expected class expression to use assignment lhs name.\nOutput: {output}"
    );
}

#[test]
fn test_commonjs_void_zero_exports_are_emitted_in_reverse_declaration_order() {
    let source = "const a = 1;\nconst b = 2;\nexport { a, b };\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("exports.b = exports.a = void 0;"),
        "unexpected output:\n{output}"
    );
}

#[test]
fn test_es_module_export_equals_erased_to_empty_export_marker() {
    let source = "var a = 10;\nexport = a;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            ..Default::default()
        },
    );

    assert!(output.contains("var a = 10;"));
    assert!(output.contains("export {};"));
    assert!(!output.contains("export default a;"));
}

#[test]
fn test_es_module_external_import_equals_erased_to_empty_export_marker() {
    let source = "import a = require(\"./server\");\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            ..Default::default()
        },
    );

    assert_eq!(output, "export {};\n");
}

#[test]
fn test_commonjs_export_equals_interface_is_erased() {
    let source = "interface C {}\nexport = C;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(!output.contains("module.exports = C"));
}

#[test]
fn test_amd_export_import_namespace_alias_emits_export_assignment() {
    let source = "namespace x { interface c {} }\nexport import a = x.c;\nvar b: a;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::AMD,
            ..Default::default()
        },
    );

    assert!(
        output.contains("exports.a = void 0;"),
        "exported import alias should be initialized in AMD output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.a = x.c;"),
        "exported import alias should assign directly to exports.\nOutput:\n{output}"
    );
}

#[test]
fn test_commonjs_export_import_namespace_alias_keeps_export_equals() {
    let source = "namespace x { interface c {} }\nexport import a = x.c;\nexport = x;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("exports.a = void 0;"),
        "exported import alias should still get CJS initialization.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.a = x.c;"),
        "exported import alias should assign directly to exports.\nOutput:\n{output}"
    );
    assert!(
        output.contains("module.exports = x;"),
        "export = should be preserved when an exported alias references the namespace.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__esModule"),
        "export = output should not include an __esModule marker.\nOutput:\n{output}"
    );
}

#[test]
fn test_for_await_of_target_es2018_preserved() {
    let source = "async function f() {\n    const iterable = [];\n    for await (const x of iterable) {\n        console.log(x);\n    }\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2018,
            ..Default::default()
        },
    );

    assert!(output.contains("for await (const x of iterable)"));
    assert!(!output.contains("__asyncValues"));
}

#[test]
fn test_for_await_of_target_es2017_downlevel_to_await() {
    let source = "const iterable = [];\nasync function f() {\n    for await (const x of iterable) {\n        console.log(x);\n    }\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2017,
            ..Default::default()
        },
    );

    assert!(output.contains("__asyncValues"));
    assert!(output.contains("for (var"));
    assert!(output.contains(".next()"));
    assert!(output.contains("await"));
    assert!(!output.contains("yield iterable_1.next()"));
}

#[test]
fn test_for_await_of_target_es2016_downlevel_to_yield() {
    let source = "const iterable = [];\nasync function f() {\n    for await (const x of iterable) {\n        console.log(x);\n    }\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2016,
            ..Default::default()
        },
    );

    assert!(output.contains("__awaiter"));
    assert!(output.contains("__asyncValues"));
    assert!(output.contains("yield"));
    assert!(output.contains(".next()"));
}

#[test]
fn test_nested_for_await_of_targets_nested_return_temps() {
    let source = "async function f() {\n    for await (const a of xs) {\n        for await (const b of ys) {\n            console.log(a, b);\n        }\n    }\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2016,
            ..Default::default()
        },
    );

    assert!(output.contains("for (var"));
    assert!(output.contains("__asyncValues"));
    assert!(output.contains("var e_1"));
    assert!(output.contains("e_2"));
    assert!(output.contains("_a ="));
}

#[test]
fn test_template_literal_closing_brace_with_whitespace() {
    // Regression test: template substitutions with whitespace padding inside
    // `${ expr }` must preserve closing `}` and backtick.
    let source = "var x = `${ null }${ 4 }`;\n";
    let output = parse_lower_print(source, PrintOptions::es6());
    assert!(
        output.contains("`${null}${4}`"),
        "expected template with closing braces, got: {output}"
    );
}

#[test]
fn test_template_literal_closing_brace_no_whitespace() {
    let source = "var x = `${null}${4}`;\n";
    let output = parse_lower_print(source, PrintOptions::es6());
    assert!(
        output.contains("`${null}${4}`"),
        "expected template with closing braces, got: {output}"
    );
}

#[test]
fn test_template_literal_tail_backtick_with_content() {
    // Ensure TemplateTail text + closing backtick are both emitted.
    let source = "var x = `hello ${ name } world`;\n";
    let output = parse_lower_print(source, PrintOptions::es6());
    assert!(
        output.contains("`hello ${name} world`"),
        "expected template with tail content and backtick, got: {output}"
    );
}

#[test]
fn test_tagged_template_closing_brace_with_whitespace() {
    let source =
        "function tag(s: TemplateStringsArray, ...a: any[]) {}\ntag `${ null }${ null }`;\n";
    let output = parse_lower_print(source, PrintOptions::es6());
    assert!(
        output.contains("tag `${null}${null}`"),
        "expected tagged template with closing braces, got: {output}"
    );
}

#[test]
fn test_template_literal_multiple_spans_mixed_whitespace() {
    // Mix of spaces and no-spaces across multiple substitutions.
    let source = "var x = `a${ 1 }b${2}c${ 3 }d`;\n";
    let output = parse_lower_print(source, PrintOptions::es6());
    assert!(
        output.contains("`a${1}b${2}c${3}d`"),
        "expected template with all closing braces, got: {output}"
    );
}

#[test]
fn test_amd_non_module_script_no_use_strict() {
    // Non-module scripts (no import/export) under AMD should NOT get "use strict".
    let source = "var x = 1;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            module: ModuleKind::AMD,
            ..Default::default()
        },
    );

    assert!(
        !output.contains("use strict"),
        "AMD non-module script should not get 'use strict', got: {output}"
    );
}

#[test]
fn test_amd_export_assignment_elides_unused_namespace_alias() {
    let source = r#"namespace M {
    export class C {}
}
import M22 = M;
export = M;"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            module: ModuleKind::AMD,
            ..Default::default()
        },
    );

    assert!(
        !output.contains("M22"),
        "Unused namespace alias should be erased from export-assignment modules.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return M;"),
        "Export assignment should still return the namespace value.\nOutput:\n{output}"
    );
}

#[test]
fn test_commonjs_module_gets_use_strict() {
    // CJS module files (with export) should get "use strict" in the preamble.
    let source = "export const x = 1;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("\"use strict\""),
        "CJS module should get 'use strict', got: {output}"
    );
}

#[test]
fn test_commonjs_non_module_no_use_strict() {
    // CJS non-module scripts should NOT get "use strict".
    let source = "var x = 1;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        !output.contains("use strict"),
        "CJS non-module script should not get 'use strict', got: {output}"
    );
}

// =========================================================================
// Enum var/let keyword tests
// =========================================================================

#[test]
fn test_top_level_enum_uses_var_at_es2015() {
    let source = "enum Color { Red, Green, Blue }";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        output.contains("var Color;"),
        "Top-level enum at ES2015 should use 'var', got: {output}"
    );
}

#[test]
fn test_enum_in_function_uses_let_at_es2015() {
    let source = "function foo() { enum E { A, B } }";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        output.contains("let E;"),
        "Enum inside function at ES2015 should use 'let', got: {output}"
    );
    assert!(
        !output.contains("var E;"),
        "Should not contain 'var E;' for block-scoped enum, got: {output}"
    );
}

#[test]
fn test_enum_in_function_uses_var_at_es5() {
    let source = "function foo() { enum E { A, B } }";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    assert!(
        output.contains("var E;"),
        "Enum inside function at ES5 should use 'var', got: {output}"
    );
}

#[test]
fn test_top_level_enum_uses_var_at_es5() {
    let source = "enum E { A, B, C }";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    assert!(
        output.contains("var E;"),
        "Top-level enum at ES5 should use 'var', got: {output}"
    );
}

#[test]
fn test_extends_optional_chain_parenthesized_downlevel() {
    // When target < ES2020, `A?.B` is lowered to a conditional expression.
    // In an `extends` clause, this must be wrapped in parens because
    // `extends` requires a LeftHandSideExpression.
    let source = r#"namespace A {
    export class B {}
}
class C1 extends A?.B {}
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    // The lowered optional chain must be wrapped in parens
    assert!(
        output.contains("extends (A === null"),
        "Lowered optional chain in extends clause should be parenthesized, got: {output}"
    );
}

#[test]
fn test_commonjs_class_export_before_static_block_iife() {
    // Regression test: exports.C = C; must appear between the class body
    // and the lowered static block IIFE, matching tsc behavior.
    let source =
        "export class C {\n    static x: number;\n    static {\n        C.x = 1;\n    }\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    // exports.C = C; must come before the static block IIFE
    let export_pos = output.find("exports.C = C;");
    let iife_pos = output.find("(() => {");
    assert!(
        export_pos.is_some() && iife_pos.is_some(),
        "Expected both exports.C = C; and IIFE in output, got: {output}"
    );
    assert!(
        export_pos.unwrap() < iife_pos.unwrap(),
        "exports.C = C; must appear before the static block IIFE, got: {output}"
    );
}

#[test]
fn test_lowered_static_block_recovered_await_emits_yield() {
    let source = "class C {\n    static {\n        await 1;\n        yield 1;\n    }\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("(() => {\n    yield 1;\n    yield 1;\n})();"),
        "Lowered static block should emit recovered await as yield, got: {output}"
    );
}

#[test]
fn test_lowered_static_block_uses_static_initializer_context() {
    let source = "class B { static a = 1; }\nclass C extends B { static c = super.a; static { this.c; super.a; } }\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("C.c = Reflect.get("),
        "Static field super access should use Reflect.get, got: {output}"
    );
    assert!(
        output.contains("(() => { _a.c; Reflect.get(_b, \"a\", _a); })();"),
        "Lowered static block should reuse static this/super aliases, got: {output}"
    );
}

// =========================================================================
// Comment skipping for erased type annotations
// =========================================================================

#[test]
fn test_var_type_annotation_comments_not_leaked() {
    // Comments inside a multi-line type annotation should be consumed when
    // the type is erased, not leaked to the emitted variable statement.
    let source = r#"var v: {
    (x: number); // inside type
    foo(): void; // also inside type
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(&parser.arena, root, PrintOptions::es6()).code;
    assert_eq!(
        output.trim(),
        "var v;",
        "Comments from inside erased type annotation should not appear in output, got: {output}"
    );
}

#[test]
fn test_var_simple_type_annotation_trailing_comment_preserved() {
    // A trailing comment on a simple type annotation line should stay on
    // the emitted variable statement (it's on the same line as `var`).
    let source = "var v: number; // keep this\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(&parser.arena, root, PrintOptions::es6()).code;
    assert!(
        output.contains("var v; // keep this"),
        "Trailing comment on same line should be preserved, got: {output}"
    );
}

#[test]
fn test_function_param_type_annotation_comments_not_leaked() {
    // Comments inside erased parameter type annotations should not appear.
    let source = r#"function foo(
    x: {
        a: number; // type member comment
    }
) { }"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(&parser.arena, root, PrintOptions::es6()).code;
    assert!(
        !output.contains("type member comment"),
        "Comments from inside erased param type should not appear, got: {output}"
    );
}

// =========================================================================
// CJS exported namespace: let/const preservation at ES2015+ targets
// =========================================================================

#[test]
fn test_cjs_exported_namespace_preserves_let_const_es2015() {
    // CJS-exported namespaces at ES2015+ should preserve let/const inside IIFE bodies.
    // Previously, these were incorrectly routed through the ES5 namespace emitter
    // which always emits `var`.
    let source = r#"export namespace N {
    let a = 1;
    const b = 2;
    var c = 3;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            module: ModuleKind::CommonJS,
            ..PrintOptions::es6()
        },
    )
    .code;
    assert!(
        output.contains("let a = 1;"),
        "CJS namespace at ES2015+ should preserve 'let', got: {output}"
    );
    assert!(
        output.contains("const b = 2;"),
        "CJS namespace at ES2015+ should preserve 'const', got: {output}"
    );
    assert!(
        output.contains("var c = 3;"),
        "CJS namespace at ES2015+ should preserve 'var', got: {output}"
    );
    // The IIFE tail should fold exports.N
    assert!(
        output.contains("exports.N = N = {}"),
        "CJS namespace should fold export into IIFE tail, got: {output}"
    );
}

#[test]
fn test_cjs_exported_namespace_uses_var_at_es5() {
    // CJS-exported namespaces at ES5 should still use `var` for everything.
    let source = r#"export namespace N {
    let a = 1;
    const b = 2;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            module: ModuleKind::CommonJS,
            ..PrintOptions::es5()
        },
    )
    .code;
    assert!(
        output.contains("var a = 1;"),
        "CJS namespace at ES5 should use 'var' for let, got: {output}"
    );
    assert!(
        output.contains("var b = 2;"),
        "CJS namespace at ES5 should use 'var' for const, got: {output}"
    );
}

#[test]
fn test_cjs_exported_namespace_reopen_declares_var_once_es5() {
    let source = r#"export namespace N {
    export class A {}
}
export namespace N {
    export class B {}
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            module: ModuleKind::CommonJS,
            ..PrintOptions::es5()
        },
    )
    .code;

    assert_eq!(
        output.matches("var N;").count(),
        1,
        "Reopened exported namespace should only declare the namespace var once.\nOutput:\n{output}"
    );
}

#[test]
fn test_comment_preserved_after_erased_type_annotation() {
    // When a type annotation is erased during emit, comments that follow
    // the annotation in trailing trivia should not be consumed.
    // Regression test: skip_comments_in_range used raw node.end which
    // extends into trailing trivia, consuming comments meant for the
    // next statement.
    let source = "var x: {\n    foo: string,\n    bar: string\n}\n\n// ASI makes this work\nvar y: {\n    foo: string\n    bar: string\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());
    assert!(
        output.contains("// ASI makes this work"),
        "Comment after erased type annotation should be preserved.\nOutput: {output}"
    );
}

#[test]
fn test_comment_preserved_after_erased_function_return_type() {
    // Comments after an erased return type annotation should not be consumed.
    let source = "function foo(): number {\n    // body comment\n    return 1;\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());
    assert!(
        output.contains("// body comment"),
        "Comment inside function body should be preserved after return type erasure.\nOutput: {output}"
    );
}

#[test]
fn test_async_arrow_destructuring_default_param_temp_var_no_collision() {
    // Regression: async arrow with a destructuring default param AND an
    // awaited call must not produce two `var _a;` hoists or two `_a = ...`
    // assignments in the same `__awaiter` scope. The temp var counter from
    // the AsyncES5Emitter must be synced back to the main emitter so the
    // destructuring prologue and the awaiter generator body use disjoint
    // temp names.
    let source = "var f = async ({x} = {x: 1}) => { return fn(await p); };\n";
    let output = parse_lower_print(source, PrintOptions::es5());

    // Sanity: output should still use the awaiter/generator pipeline.
    assert!(
        output.contains("__awaiter"),
        "Expected __awaiter in output:\n{output}"
    );

    // Each underscore-prefixed temp identifier should have at most one
    // `var <name>` declaration inside a single __awaiter callback. Prior
    // to the fix, the destructuring prologue and the generator body both
    // chose `_b`, producing both `var _b;` (hoisted) and a separate
    // `var _b = _a === void 0 ? ...` in the same scope.
    for letter in b'a'..=b'z' {
        let name = format!("_{}", letter as char);
        let var_decl = format!("var {name}");
        let count = output.matches(var_decl.as_str()).count();
        assert!(
            count <= 1,
            "Expected at most one `var {name}` declaration, got {count}.\n\
             This indicates the AsyncES5 temp var counter was not synced back \
             to the main emitter, causing the destructuring prologue and the \
             awaiter body to collide on the same temp name.\nOutput:\n{output}"
        );
    }
}
