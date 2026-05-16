use super::*;
use crate::output::source_writer::DelimiterKind;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::node::NodeArena;

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

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "structured delimiter helpers left 1 unclosed delimiter")]
fn finish_asserts_structured_delimiters_are_balanced() {
    let arena = NodeArena::new();
    let mut printer = Printer::new(&arena, PrintOptions::default());
    printer
        .inner
        .writer
        .write_open_delimiter(DelimiterKind::Paren);

    let _ = printer.finish();
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
fn recovered_arrow_conditional_tail_emits_branch_statements() {
    let source = "(a?) => { return a; } ? (b)=>(c)=>81 : (c)=>(d)=>82;\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("(a) => { return a; };"),
        "The block-bodied arrow should emit as the first recovered expression statement.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(b) => (c) => 81;"),
        "The invalid conditional true branch should remain emit-visible.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(c) => (d) => 82;"),
        "The invalid conditional false branch should remain emit-visible.\nOutput:\n{output}"
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
fn optional_parameter_missing_initializer_skips_question_after_trivia() {
    let source = "function f(a ? = ) {}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert_eq!(output, "function f(a = ) { }\n");
}

#[test]
fn es5_param_destructuring_prologue_keeps_function_body_let_name() {
    let source = "let foo = \"\";\nfunction f({ [foo]: bar }: any[]) {\n    let foo = 2;\n}\n";
    let output = parse_lower_print(source, PrintOptions::es5());

    assert!(
        output.contains("var _b = foo, bar = _a[_b];\n    var foo = 2;"),
        "Function-body let declarations should use the function scope opened by the ES5 parameter prologue.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var foo_1 = 2;"),
        "Function-body let should not be renamed against the outer foo.\nOutput:\n{output}"
    );
}

#[test]
fn object_spread_recovery_keeps_trailing_empty_object() {
    let source = "let o9 = { ...matchMedia() { }};\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("let o9 = Object.assign({}, matchMedia()), {};"),
        "Malformed object spread should preserve the recovered trailing empty object.\nOutput:\n{output}"
    );
}

#[test]
fn optional_instantiation_recovery_emits_optional_call() {
    let source = "declare let f: { <T>(): T };\nconst b1 = f?.<number>;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("const b1 = f === null || f === void 0 ? void 0 : f();"),
        "Malformed optional instantiation should recover as an optional call.\nOutput:\n{output}"
    );
}

#[test]
fn jsx_numeric_tag_recovery_preserves_tail() {
    let source =
        "const x = \"oops\";\nconst a = + <number> x;\nconst b = + <> x;\nconst c = + <1234> x;\n";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2015,
            jsx: JsxEmit::Preserve,
            ..Default::default()
        },
    )
    .code;

    assert!(
        output.contains("const c = + < />1234> x;"),
        "Malformed numeric JSX tag should preserve the recovered numeric tail.\nOutput:\n{output}"
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
fn recovered_template_object_property_name_emits_as_recovered_statements() {
    let source = "var x = {\n    `abc${ 123 }def${ 456 }ghi`: 321\n}";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert_eq!(output, "var x = {} `abc${123}def${456}ghi`;\n321;\n");
}

#[test]
fn recovered_template_module_names_emit_as_recovered_statements() {
    let source = "declare module `M1` {\n}\n\ndeclare module `M${2}` {\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert_eq!(
        output,
        "declare;\nmodule `M1`;\n{\n}\ndeclare;\nmodule `M${2}`;\n{\n}\n"
    );
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

/// `obj.prop **= rhs` must capture the receiver in a temp before
/// re-reading the property, even when the receiver is a simple
/// identifier. The lowered shape `obj.prop = Math.pow(obj.prop, rhs)`
/// would re-evaluate the receiver-name lookup twice; tsc avoids this
/// with `(_a = obj).prop = Math.pow(_a.prop, rhs)` so any future
/// getter/Proxy on `obj` would only fire once. This locks that in.
#[test]
fn test_exponentiation_assignment_property_access_temps_simple_base() {
    let source = "let x3 = { a: 2 };\nx3.a **= 4;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("(_a = x3).a = Math.pow(_a.a, 4)"),
        "Property-access `**=` must temp the receiver even for a simple identifier base.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("x3.a = Math.pow(x3.a"),
        "Property-access `**=` must not re-emit the receiver expression twice.\nOutput:\n{output}"
    );
}

#[test]
fn test_optional_call_downlevel_to_conditional() {
    let source = "const fn = () => 1;\nconst obj = { m() { return this; } };\nfn?.();\nobj?.m();\nobj.m?.();\nobj?.m?.();\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2019,
            ..Default::default()
        },
    );
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
fn test_commonjs_empty_named_import_is_elided() {
    let source = "import {} from \"./side\";\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(!output.contains("require(\"./side\");"));
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
fn commonjs_preamble_stays_after_source_prologue_strings() {
    let source = "\"hey!\";\n\" use strict \";\nexport function f() {}\n";
    let output = parse_lower_print(source, PrintOptions::es5_commonjs());

    let strict_pos = output
        .find("\"use strict\";")
        .expect("CJS output should include synthetic use strict");
    let first_prologue_pos = output
        .find("\"hey!\";")
        .expect("First source prologue should be emitted");
    let second_prologue_pos = output
        .find("\" use strict \";")
        .expect("Second source prologue should be emitted");
    let marker_pos = output
        .find("Object.defineProperty(exports, \"__esModule\"")
        .expect("CJS output should include __esModule marker");
    let export_pos = output
        .find("exports.f = f;")
        .expect("Function export should be hoisted");
    let function_pos = output
        .find("function f()")
        .expect("Function declaration should be emitted");

    assert!(strict_pos < first_prologue_pos, "Output:\n{output}");
    assert!(
        first_prologue_pos < second_prologue_pos,
        "Output:\n{output}"
    );
    assert!(second_prologue_pos < marker_pos, "Output:\n{output}");
    assert!(marker_pos < export_pos, "Output:\n{output}");
    assert!(export_pos < function_pos, "Output:\n{output}");
}

#[test]
fn test_async_generator_dynamic_import_nested_yield() {
    let source = "async function* foo() {\n    import((await import(yield \"foo\")).default);\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("Promise.resolve(`${yield yield __await(\"foo\")}`)"),
        "Nested dynamic import yield should be awaited for async generator lowering.\nOutput:\n{output}"
    );
}

#[test]
fn test_invalid_export_throw_elides_recovery_export_and_keeps_strict() {
    let source = "throw;\n\nexport throw null;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            ..Default::default()
        },
    );

    assert_eq!(output, "\"use strict\";\nthrow ;\nthrow null;\n");
}

#[test]
fn test_nested_namespace_extends_parent_export_when_name_conflicts() {
    let source = "declare namespace A.B.C {\n    class B {\n    }\n}\n\nnamespace A.B {\n    export class EventManager {\n        id: number;\n    }\n}\n\nnamespace A.B.C {\n    export class ContextMenu extends EventManager {\n        name: string;\n    }\n}\n";
    let output = parse_lower_print(source, PrintOptions::default());

    assert!(
        output.contains("class ContextMenu extends B.EventManager"),
        "Nested namespace heritage should qualify parent namespace export.\nOutput:\n{output}"
    );
}

#[test]
fn test_namespace_heritage_prefers_current_block_class_over_parent_export() {
    let source = "namespace M {\n    export namespace C { export function f() {} }\n}\nnamespace M.P {\n    export class C {}\n    export class E extends C {}\n}\n";
    let output = parse_lower_print(source, PrintOptions::default());

    assert!(
        output.contains("class E extends C"),
        "Heritage references should prefer a class declared in the current namespace block.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("class E extends M.C"),
        "Parent namespace export should not shadow the current block's local class binding.\nOutput:\n{output}"
    );
}

#[test]
fn test_nested_namespace_qualifies_parent_class_from_prior_block() {
    // Regression for #2521 review: when a parent namespace is reopened in a
    // later block that contains a nested namespace, references to a class
    // declared in an EARLIER block of the same parent namespace must qualify
    // as `Parent.Foo` (the earlier block's IIFE has exited, so the class is
    // only reachable via the namespace object). Splitting class/fn/enum
    // exports into a separate map originally regressed this cross-block case
    // because parent_exports only read the var-only prior-exports map.
    let source = "namespace A { export class Foo {} }\nnamespace A { export namespace B { console.log(Foo); } }\n";
    let output = parse_lower_print(source, PrintOptions::default());

    assert!(
        output.contains("console.log(A.Foo)"),
        "Nested namespace inside reopened parent block should qualify class \
         from prior parent block as `A.Foo`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("console.log(Foo)"),
        "Bare `Foo` reference must not appear — class lives on namespace \
         object after the prior IIFE exited.\nOutput:\n{output}"
    );
}

#[test]
fn test_nested_namespace_does_not_qualify_parent_class_in_same_block() {
    // Companion to test_nested_namespace_qualifies_parent_class_from_prior_block:
    // ensure the same-block case (the fix from PR #2521) still works. Within a
    // single namespace block, the parent's class is in lexical scope of the
    // surrounding IIFE, so a nested namespace must reference it bare.
    let source =
        "namespace A {\n    export class Foo {}\n    export namespace B { console.log(Foo); }\n}\n";
    let output = parse_lower_print(source, PrintOptions::default());

    assert!(
        output.contains("console.log(Foo)"),
        "Same-block nested namespace should reference parent class without \
         qualification (lexical scope of surrounding IIFE).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("console.log(A.Foo)"),
        "Same-block nested namespace must NOT qualify parent class — that \
         would break tsc parity.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_reference_to_later_dotted_child_is_qualified() {
    let source = "namespace TypeScript {\n    export class PositionedElement {\n        childIndex() {\n            return Syntax.childIndex();\n        }\n    }\n}\nnamespace TypeScript.Syntax {\n    export function childIndex() { }\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("return TypeScript.Syntax.childIndex();"),
        "Reference to a dotted child namespace declared in a later block should \
         be qualified through the parent namespace.\nOutput:\n{output}"
    );
}

#[test]
fn nested_namespace_qualifies_grandparent_export_when_name_collides() {
    let source = "namespace M {\n    export var x = 3;\n    namespace m4 {\n        namespace M {\n            var p = x;\n        }\n    }\n}\n";
    let output = parse_lower_print(source, PrintOptions::default());

    assert!(
        output.contains("var p = M_1.x;"),
        "Nested colliding namespace should qualify grandparent export through \
         the outer IIFE parameter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var p = x;"),
        "Bare `x` would resolve against the nested namespace scope, not the \
         exported grandparent value.\nOutput:\n{output}"
    );
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
fn es2015_computed_instance_field_side_effects_fold_into_method_name() {
    let source = "class C {\n    [Symbol.iterator] = 0;\n    [Symbol.unscopables]: number;\n    [Symbol.toPrimitive]() { }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("this[_a] = 0;"),
        "expected constructor to use hoisted computed field temp:\n{output}"
    );
    assert!(
        output.contains("[(_a = Symbol.iterator, Symbol.unscopables, Symbol.toPrimitive)]() { }"),
        "expected prior computed field expressions to fold into the computed method name:\n{output}"
    );
    assert!(
        !output.contains("_a = Symbol.iterator, Symbol.unscopables;"),
        "unexpected trailing computed field side-effect expression:\n{output}"
    );
}

#[test]
fn commonjs_local_undefined_export_skips_redundant_assignment() {
    let source = "var undefined;\nexport { undefined };\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("exports.undefined = void 0;\nvar undefined;"),
        "expected undefined export preamble and local declaration:\n{output}"
    );
    assert!(
        !output.contains("exports.undefined = undefined;"),
        "unexpected redundant undefined export assignment:\n{output}"
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
fn test_commonjs_export_import_type_only_namespace_identifier_is_erased() {
    let source = "export namespace C { export interface I {} }\nexport import v = C;\nexport namespace M { export var w: v.I; }\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("exports.M = void 0;"),
        "runtime namespace export should still be initialized.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.v"),
        "type-only import-equals alias should not emit a CJS export.\nOutput:\n{output}"
    );
}

#[test]
fn amd_known_declaration_file_without_bang_module_is_stripped() {
    let declarations = r#"declare module "regular" {
    export const value: number;
}
"#;
    let source = r#"/// <reference path="types.d.ts"/>

import value from "loader!module";
export const y = value;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut declaration_file = parser.arena.source_files[0].clone();
    declaration_file.file_name = "types.d.ts".to_string();
    declaration_file.text = std::sync::Arc::from(declarations);
    declaration_file.is_declaration_file = true;
    parser.arena.source_files.push(declaration_file);

    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            module: ModuleKind::AMD,
            ..Default::default()
        },
    )
    .code;

    assert!(
        !output.contains("/// <reference"),
        "Known .d.ts files that do not declare the imported bang module should not be preserved by the source-text fallback.\nOutput:\n{output}"
    );
}

#[test]
fn amd_missing_declaration_fallback_ignores_exported_string_with_bang() {
    let source = r#"/// <reference path="missing.d.ts"/>
export const msg = "Hello!";
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            module: ModuleKind::AMD,
            ..Default::default()
        },
    );

    assert!(
        !output.contains("/// <reference"),
        "Fallback bang-module detection should ignore non-module export declarations with string literals.\nOutput:\n{output}"
    );
}

#[test]
fn consecutive_triple_slash_refs_emit_together_before_cjs_preamble() {
    let source = r#"/// <reference path="O.d.ts" />
/// <reference path="O2.d.ts" />

import { x } from "M";
export const y = x;
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    let o_idx = output
        .find("/// <reference path=\"O.d.ts\" />")
        .expect("O.d.ts ref should be emitted");
    let o2_idx = output
        .find("/// <reference path=\"O2.d.ts\" />")
        .expect("O2.d.ts ref should be emitted");
    let preamble_idx = output
        .find("Object.defineProperty(exports")
        .expect("CJS preamble should be emitted");

    assert!(
        o_idx < preamble_idx,
        "First triple-slash ref must appear before __esModule preamble.\nOutput:\n{output}"
    );
    assert!(
        o2_idx < preamble_idx,
        "Second triple-slash ref must appear before __esModule preamble.\nOutput:\n{output}"
    );
    assert!(
        o_idx < o2_idx,
        "Triple-slash refs must preserve source order.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_function_expression_parameter_shadows_exported_name_es2015() {
    let source = r#"namespace Foo {
    export function a() {}
    export const fn = function(a: number) {
        return a;
    };
}"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("return a;"),
        "Function expression body should reference its parameter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return Foo.a;"),
        "Function expression parameter should shadow namespace exported names.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_arrow_parameter_shadows_exported_name_es2015() {
    let source = r#"namespace Foo {
    export function a() {}
    export const arrow = (a: number) => a;
}"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("Foo.arrow = (a) => a;"),
        "Arrow concise body should reference its parameter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("=> Foo.a"),
        "Arrow parameter should shadow namespace exported names.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_function_default_parameter_shadows_exported_name_es2015() {
    let source = r#"namespace Foo {
    export let a = 10;
    export const fn = function(a: number, b = a) {
        return b;
    };
}"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("function (a, b = a)"),
        "Function default parameter should reference the prior parameter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("b = Foo.a"),
        "Function default parameter should not qualify a shadowed namespace export.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_arrow_default_parameter_shadows_exported_name_es2015() {
    let source = r#"namespace Foo {
    export let a = 10;
    export const arrow = (a: number, b = a) => b;
}"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("Foo.arrow = (a, b = a) => b;"),
        "Arrow default parameter should reference the prior parameter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("b = Foo.a"),
        "Arrow default parameter should not qualify a shadowed namespace export.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_arrow_default_prologue_shadows_exported_name_es2016() {
    let source = r#"namespace Foo {
    export function a() {
        return 10;
    }
    declare function complex(): number;
    export const arrow = (a: () => number | undefined, b = a() ?? complex()) => a();
}"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2016,
            ..Default::default()
        },
    );

    assert!(
        output.contains("b = (_a = a()) !== null"),
        "Default prologue should reference the parameter before the fallback.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return a();") && !output.contains("return Foo.a();"),
        "Default prologue arrow body should reference the shadowing parameter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Foo.a()") && !output.contains("= Foo.a"),
        "Default prologue should not qualify shadowed namespace parameter references.\nOutput:\n{output}"
    );
}

#[test]
fn static_field_class_expression_in_arrow_parameter_default_lowers_es2015() {
    let source = "((b = class { static x = 1 }) => {})();";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var __setFunctionName"),
        "static class default parameter should request the named-evaluation helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("((b) => {\n    var _a;\n    if (b === void 0) { b = (_a = class")
            && output.contains("__setFunctionName(_a, \"b\")")
            && output.contains("_a.x = 1,"),
        "ES2015 arrow default should lower to a body prologue with a scoped class-expression alias.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("((b = (_a = class"),
        "ES2015 output must not keep the transformed class expression inside the parameter list.\nOutput:\n{output}"
    );
}

#[test]
fn static_field_class_expression_in_parameter_default_uses_es5_comma_alias() {
    let source = "((b = class { static x = 1 }) => {})();";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var __setFunctionName"),
        "ES5 static class default parameter should request the named-evaluation helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function (b) {\n    var _a;\n    if (b === void 0) { b = (_a = /** @class */ (function () {")
            && output.contains("function class_1()")
            && output.contains("__setFunctionName(_a, \"b\")")
            && output.contains("_a.x = 1,"),
        "ES5 default parameter should use the static-field comma alias form.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("b = (function () {"),
        "ES5 output must not wrap the class expression in a nested IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_decorated_anonymous_default_class_static_field_sets_default_name() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare function dec<T>(target: T): T;\n@dec\nexport default class {\n    static y = 1;\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        legacy_decorators: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __setFunctionName ="),
        "Lowered static field on anonymous decorated default class must request __setFunctionName.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _a;"),
        "The class value alias should be hoisted before the default class assignment.\nOutput:\n{output}"
    );
    assert!(
        output.contains("let default_1 = _a = class"),
        "Anonymous decorated default class should assign both the export binding and function-name alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_a, \"default\");")
            && output.contains("default_1.y = 1;")
            && output.contains("default_1 = __decorate([")
            && output.contains("export default default_1;"),
        "Static initialization, decoration, and default export should follow tsc's statement order.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__setFunctionName(_a, \"default_1\")"),
        "The runtime function name is the default export name, not the synthetic binding.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_constructor_param_decorator_static_self_reference_uses_alias() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare const IFoo: any;\nclass BulkEditPreviewProvider {\n    static readonly Schema = 'vscode-bulkeditpreview';\n    static emptyPreview = { scheme: BulkEditPreviewProvider.Schema };\n    constructor(@IFoo private readonly _modeService: IFoo) { }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2018,
        legacy_decorators: true,
        no_emit_helpers: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var BulkEditPreviewProvider_1;"),
        "Constructor parameter decorators that reassign the class need a stable alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "let BulkEditPreviewProvider = BulkEditPreviewProvider_1 = class BulkEditPreviewProvider"
        ),
        "The class expression should initialize both the public binding and the stable alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "BulkEditPreviewProvider.emptyPreview = { scheme: BulkEditPreviewProvider_1.Schema };"
        ),
        "Static self-references must read from the pre-decoration class alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("BulkEditPreviewProvider = BulkEditPreviewProvider_1 = __decorate(["),
        "The class decorator assignment must keep the alias tracking the decorated class value.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_late_property_decorator_recovers_onto_following_method() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var decorator: any;\nclass Foo {\n    private prop @decorator\n    foo() {\n        return 0;\n    }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ESNext,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("prop;"),
        "The malformed property should still emit as a field.\nOutput:\n{output}"
    );
    assert!(
        output.contains("], Foo.prototype, \"foo\", null);"),
        "The late decorator should recover onto the following method.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:type\", Function)"),
        "Recovered method decorator should still emit metadata.\nOutput:\n{output}"
    );
}

#[test]
fn system_exported_object_binding_non_identifier_property_uses_destructuring_path() {
    let source = r#"declare const obj: any;
export let { "foo": bar } = obj;
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::System,
            ..Default::default()
        },
    );

    assert!(
        output.contains("exports_1(\"bar\", bar = obj[\"foo\"]);"),
        "String-literal binding property names should access the source property, not the binding name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("obj.bar"),
        "System binding shortcut must not use the binding name as the property name.\nOutput:\n{output}"
    );
}

#[test]
fn system_exported_object_binding_bracket_access_does_not_add_numeric_dot() {
    let source = r#"export let { "foo": bar } = 42.5;
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::System,
            ..Default::default()
        },
    );

    assert!(
        output.contains("exports_1(\"bar\", bar = 42.5[\"foo\"]);"),
        "Bracket access after numeric literal should not emit an extra dot.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("42.5.["),
        "Extra numeric-literal dot before bracket access is invalid JS.\nOutput:\n{output}"
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
fn test_invalid_interface_without_name_recovers_body_text() {
    let source = "interface { }\ninterface interface{ }\ninterface & { }\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("interface;\n{ }"),
        "missing interface name should recover the interface token and body.\nOutput:\n{output}"
    );
    assert!(
        output.contains("interface & {};"),
        "invalid interface ampersand statement should still be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("interface interface"),
        "invalid identifier named interface should stay erased.\nOutput:\n{output}"
    );
}

#[test]
fn test_unterminated_empty_switch_recovers_following_class() {
    let source = "class C {\n  constructor() {\n    switch (e) {\n\nclass D {\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("switch (e) {\n        }\n        class D {\n        }"),
        "unterminated empty switch should recover following class declaration.\nOutput:\n{output}"
    );
}

#[test]
fn test_unterminated_empty_switch_recovers_extending_class_with_inline_member() {
    let source = "declare const x: number;\ndeclare class B {}\nswitch (x) {\nclass C extends B { static value = 1 }\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2022,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("switch (x) {\n}\nclass C extends B {\n    static value = 1;\n}"),
        "unterminated empty switch should recover following class with heritage and inline members.\nOutput:\n{output}"
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
    // After ESM for-await alignment, hoisted catch temps are coalesced into a
    // single `var` declaration alongside other top-of-function temps, matching
    // TypeScript's emit. Verify both temps still appear in such a declaration.
    assert!(output.contains("e_1"));
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
fn super_tagged_template_with_type_arguments_preserves_recovery_dot() {
    let source =
        "class Base {}\nclass Derived extends Base { constructor() { super<string> `value`; } }\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("super. `value`;"),
        "Recovered super tagged template should preserve tsc's property-access dot.\nOutput:\n{output}"
    );
}

// `import { css } from "lib"; css `...`;` lowered to CommonJS becomes
// `(0, lib_1.css) `...`;` so the tagged-template invocation does not bind
// `this` to the imported module namespace object. Without the `(0, ...)`
// wrapper, `lib_1.css `...`` would receive `lib_1` as `this`. Mirrors
// tsc's `inlineJsxFactoryDeclarations` and
// `jsxImportSourceNonPragmaComment` baselines.
#[test]
fn cjs_imported_function_tagged_template_wraps_in_zero_comma() {
    let source = "import { css } from \"lib\";\nconst a = css `red`;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        output.contains("(0, lib_1.css) `red`"),
        "Tagged template with a CJS-imported tag must wrap in `(0, lib_1.css)` to detach `this`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("lib_1.css `red`"),
        "Bare `lib_1.css` tag must not be emitted — would bind `this` to the namespace object.\nOutput:\n{output}"
    );
}

/// JSX attribute names are property keys on the synthesized props
/// object — they must not pick up the CJS-named-import substitution
/// that `emit_identifier` applies to value identifiers. Without this,
/// `<input css={...}/>` reads as `<input lib_1.css={...}/>` after
/// CJS lowering with `--jsx preserve`, which is invalid JSX.
#[test]
fn jsx_attribute_name_skips_cjs_named_import_substitution() {
    let source = "import { css } from \"lib\";\nconst foo = <input css={42}/>;\n";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let output = lower_and_print(
        &parser.arena,
        root,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            jsx: JsxEmit::Preserve,
            ..Default::default()
        },
    )
    .code;

    assert!(
        output.contains("<input css={42}"),
        "JSX attribute name `css` must be emitted bare even when the same identifier is a CJS-imported value.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("<input lib_1.css="),
        "JSX attribute name must not be substituted to `lib_1.css`.\nOutput:\n{output}"
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
fn erased_computed_class_fields_emit_native_static_block_side_effects() {
    let source = r#"declare const s: unique symbol;
declare namespace N { const s: unique symbol; }
class C {
    static [s]: "a";
    static [N.s]: "b";
    [s]: "a";
    [N.s]: "b";
}
"#;
    let output = parse_lower_print(source, PrintOptions::default());

    assert!(
        output.contains("class C {\n    static { N.s, N.s; }\n}"),
        "Erased computed class field side effects should stay inside a native static block.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("}\nN.s;"),
        "Erased computed class field side effects should not be emitted after the class.\nOutput:\n{output}"
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
fn variable_initializer_line_comment_indents_initializer() {
    let output = parse_lower_print("var x = // c\n1;\n", PrintOptions::es6());

    assert!(
        output.contains("var x = // c\n 1;"),
        "Initializer after a line comment should keep tsc's single-space continuation indentation.\nOutput:\n{output}"
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

/// `this.#field ??= rhs` on a private field must lower through the
/// `__classPrivateFieldSet(get() ?? rhs)` pattern, mirroring the existing
/// `+=`/`-=`/etc. compound-assignment lowering. Without this, the helper
/// emit produces `__classPrivateFieldGet(...) ??= rhs` — invalid JS, since
/// `??=` cannot apply to a function call. Mirrors tsc's emit for issue
/// `microsoft/TypeScript#61109`.
#[test]
fn private_field_nullish_assign_lowers_to_set_get_nullish_rhs() {
    let source = "class Cls {\n  #privateProp: number | undefined;\n  problem() {\n    this.#privateProp ??= 20;\n  }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("__classPrivateFieldSet(this, _Cls_privateProp, __classPrivateFieldGet(this, _Cls_privateProp, \"f\") ?? 20, \"f\")"),
        "Private-field `??=` must lower to set(get() ?? rhs).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("??="),
        "Lowered output must not still contain a `??=` operator.\nOutput:\n{output}"
    );
}

/// When the RHS of `??=`/`||=`/`&&=` on a private field is a
/// conditional expression, the lowered `get() <op> rhs` must wrap the
/// conditional in parens. `??`, `||`, and `&&` all bind tighter than the
/// conditional operator, so `get() ?? a ? b : c` would otherwise reparse
/// as `(get() ?? a) ? b : c` and silently change semantics.
#[test]
fn private_field_nullish_assign_parenthesizes_conditional_rhs() {
    let source = "class Cls {\n  #privateProp: number | undefined;\n  problem() {\n    this.#privateProp ??= false ? noop() : 20;\n  }\n}\nfunction noop(): number { return 0; }\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("?? (false ? noop() : 20)"),
        "Conditional RHS of `??=` must be parenthesized to preserve precedence.\nOutput:\n{output}"
    );
}

/// `||=` on a private field follows the same lowering shape as `??=`.
/// Locks in coverage so a future refactor of the compound-assignment
/// list can't regress one operator while leaving the others working.
#[test]
fn private_field_logical_or_assign_lowers_to_set_get_or_rhs() {
    let source =
        "class Cls {\n  #flag: boolean = false;\n  toggle() {\n    this.#flag ||= true;\n  }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("__classPrivateFieldSet(this, _Cls_flag, __classPrivateFieldGet(this, _Cls_flag, \"f\") || true, \"f\")"),
        "Private-field `||=` must lower to set(get() || rhs).\nOutput:\n{output}"
    );
}

/// `&&=` on a private field follows the same lowering shape as `??=`.
#[test]
fn private_field_logical_and_assign_lowers_to_set_get_and_rhs() {
    let source =
        "class Cls {\n  #flag: boolean = true;\n  guard() {\n    this.#flag &&= false;\n  }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("__classPrivateFieldSet(this, _Cls_flag, __classPrivateFieldGet(this, _Cls_flag, \"f\") && false, \"f\")"),
        "Private-field `&&=` must lower to set(get() && rhs).\nOutput:\n{output}"
    );
}

/// Regression: a nested namespace's name lives in the parent IIFE's
/// function scope, not at file scope. So a *file-scope* namespace with
/// the same name must still receive its own `var` declaration. The
/// lowering pass tracks `declared_names` to suppress duplicate `var`
/// emits, but the set must reset when entering and exiting a namespace
/// body: names declared inside a nested IIFE don't leak out.
#[test]
fn nested_namespace_name_does_not_suppress_outer_var_declaration() {
    let source = "namespace m1 {\n    namespace m2 {\n        export var p = 1;\n    }\n}\nnamespace m2 {\n    export var q = 2;\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    // Both the m1.m2 IIFE and the file-scope m2 IIFE need their own
    // `var m2;` preamble because each lives in a distinct scope.
    let var_count = output.matches("var m2;").count();
    assert_eq!(
        var_count, 2,
        "Each scope-local `m2` namespace needs its own `var m2;`. Found {var_count}.\nOutput:\n{output}"
    );
}

/// Counterpart: same-named namespace *reopened* at the same scope must
/// declare `var` only once. (Standard merging.)
#[test]
fn reopened_same_scope_namespace_declares_var_only_once() {
    let source =
        "namespace m1 {\n    export var p = 1;\n}\nnamespace m1 {\n    export var q = 2;\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    let var_count = output.matches("var m1;").count();
    assert_eq!(
        var_count, 1,
        "Reopened namespace at same scope should declare `var m1;` once. Found {var_count}.\nOutput:\n{output}"
    );
}

/// Regression: a same-named inner declaration that is `declare`-ambient is
/// erased at emit, so it must not trigger renaming of the namespace IIFE
/// parameter. tsc emits `(function (M) { ... })`, not `(function (M_1) { ... })`,
/// for `namespace M { export declare namespace M { } }`.
#[test]
fn namespace_iife_param_not_renamed_when_inner_same_name_is_declare() {
    let source = "namespace M {\n    export declare var x;\n    export declare function f();\n    export declare class C { }\n    export declare enum E { }\n    export declare namespace M { }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("(function (M) {"),
        "Declare-only inner `M` must not trigger IIFE param renaming.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(function (M_1)"),
        "IIFE param must not be renamed to `M_1` when the only same-name binding is ambient.\nOutput:\n{output}"
    );
}

/// Counterpart: a *concrete* inner declaration with the same name DOES
/// require IIFE-param renaming (so the outer-name reference and inner-name
/// reference don't collide).
#[test]
fn namespace_iife_param_renamed_when_inner_same_name_is_concrete() {
    let source = "namespace M {\n    export class M { foo() {} }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("M_1") || !output.contains("(function (M) {"),
        "Concrete same-name inner declaration must rename IIFE param.\nOutput:\n{output}"
    );
}

/// Regression: `export var [a, b] = init;` inside a namespace must lower
/// to a temp + indexed comma assignments — `var _a; _a = init, M.a =
/// _a[0], M.b = _a[1];`. The pre-fix emit was `M.a = init, M.b = init`
/// which evaluates the initializer twice and assigns the whole array
/// to each member.
#[test]
fn namespace_exported_array_destructuring_lowers_to_temp_and_indices() {
    let source = "namespace M {\n    export var [a, b] = [1, 2];\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _a;"),
        "Destructuring lowering must declare a temp `_a`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = [1, 2], M.a = _a[0], M.b = _a[1];"),
        "Array destructuring lowering must assign init once, then index.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("M.a = [1, 2], M.b = [1, 2];"),
        "Pre-fix shape (initializer evaluated per binding) must not appear.\nOutput:\n{output}"
    );
}

/// Object-pattern counterpart: keys are accessed by name, not index.
#[test]
fn namespace_exported_object_destructuring_lowers_to_temp_and_keys() {
    let source = "function f() { return { a4: 1, b4: 2, c4: 3 }; }\nnamespace m {\n    export var { a4, b4, c4 } = f();\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _a;"),
        "Destructuring lowering must declare a temp `_a`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = f(), m.a4 = _a.a4, m.b4 = _a.b4, m.c4 = _a.c4;"),
        "Object destructuring lowering must assign init once, then access by key.\nOutput:\n{output}"
    );
}

/// Object-pattern with rename: `{ x: a }` → key `x`, target `M.a`.
#[test]
fn namespace_exported_object_destructuring_rename_uses_property_name() {
    let source =
        "function f() { return { x: 1 }; }\nnamespace m {\n    export var { x: a } = f();\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("_a = f(), m.a = _a.x;"),
        "Renamed object binding must read source key but assign to renamed target.\nOutput:\n{output}"
    );
}

/// Instantiation expressions strip the type arguments and wrap the
/// expression in parens (`fx<T>` → `(fx)`). The empty-arg parser-recovery
/// shape `fx<>` has no real arguments, so tsc emits the bare expression
/// without parens (`fx<>` → `fx`).
#[test]
fn instantiation_expression_with_args_wraps_in_parens() {
    let source = "declare function fx<T>(x: T): T;\nfunction f1() {\n    let f1 = fx<string>;\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("let f1 = (fx);"),
        "Non-empty instantiation expression must wrap the expression in parens.\nOutput:\n{output}"
    );
}

#[test]
fn instantiation_expression_with_empty_args_emits_bare() {
    let source = "declare function fx<T>(x: T): T;\nfunction f1() {\n    let f0 = fx<>;\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("let f0 = fx;"),
        "Empty type-argument list must emit the bare expression with no parens.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("let f0 = (fx);"),
        "Empty type-argument list must not retain the wrapping parens.\nOutput:\n{output}"
    );
}

/// Regression: `(({}) as any).foo` was emitting `(({}).foo)` — wrapping
/// the entire property access in extra outer parens because the
/// "object-literal access" emitter unconditionally wrote `(` and `)`
/// even when the inner emit was already producing `({})` (from the
/// nested `ParenthesizedExpression`). tsc emits `({}).foo`.
#[test]
fn property_access_on_paren_cast_paren_object_literal_emits_single_paren() {
    let source = "interface T {}\n(({}) as any as T).foo;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("({}).foo"),
        "Receiver should be `({{}})` with `.foo` suffix outside the parens.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(({}).foo)"),
        "Outer parens around the property access are redundant when the receiver is already parenthesized.\nOutput:\n{output}"
    );
}

#[test]
fn erased_object_literal_access_does_not_wrap_return_expression() {
    let source = r#"
function prop() {
    return ({ a: 1 } as { a: number }).a;
}
function elem(key: string) {
    return ({ a: 1 } as Record<string, number>)[key];
}
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("return { a: 1 }.a;"),
        "Return property access should not keep type-erasure parens.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return { a: 1 }[key];"),
        "Return element access should not keep type-erasure parens.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return ({ a: 1 }.a);") && !output.contains("return ({ a: 1 }[key]);"),
        "Return expressions should not be wrapped like statement expressions.\nOutput:\n{output}"
    );
}

#[test]
fn erased_object_literal_access_wraps_statement_expression() {
    let source = r#"
({ a: 1 } as { a: number }).a;
({ a: 1 } as Record<string, number>)["a"];
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("({ a: 1 }.a);"),
        "Statement property access must stay parenthesized to avoid parsing as a block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("({ a: 1 }[\"a\"]);"),
        "Statement element access must stay parenthesized to avoid parsing as a block.\nOutput:\n{output}"
    );
}

/// Regression: `export default (X as T)` where `X` is a class or function
/// expression. The parens only existed to delimit the type cast; after
/// erasure they look removable, but stripping them silently changes the
/// export from "default-export an expression" to "default-export a
/// declaration". tsc preserves the parens.
#[test]
fn export_default_paren_class_expression_with_cast_keeps_parens() {
    let source = "export default (class Foo {} as any);\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("export default (class Foo {"),
        "Parens around the class expression must be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export default class Foo {"),
        "Stripping parens would change the export shape from expression to declaration.\nOutput:\n{output}"
    );
}

/// Counterpart: `export default class Foo {}` (no parens, no cast) is a
/// class declaration export and stays unchanged.
#[test]
fn export_default_class_declaration_unchanged() {
    let source = "export default class Foo {}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("export default class Foo"),
        "Bare default-class export should not gain parens.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export default (class Foo"),
        "Bare default-class export must not be wrapped in parens.\nOutput:\n{output}"
    );
}

/// Regression: `({ foo, bar } = foo)` reassigns the same identifier on
/// both sides. Inlining as `(foo = foo.foo, bar = foo.bar)` reads
/// `foo.bar` AFTER `foo` has been clobbered. tsc captures the RHS in a
/// temp first: `_a = foo, foo = _a.foo, bar = _a.bar`.
#[test]
fn es5_assignment_destructuring_reassigning_rhs_uses_temp() {
    let source = "var foo: any = { foo: 1, bar: 2 };\nvar bar: any;\n({ foo, bar } = foo);\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("(_a = foo, foo = _a.foo, bar = _a.bar);"),
        "RHS reassigned by LHS must capture in `_a` first.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(foo = foo.foo, bar = foo.bar);"),
        "Direct inline reads the clobbered `foo` for the second access.\nOutput:\n{output}"
    );
}

/// Same hazard for `var { foo, baz } = foo;` — must lower to
/// `var _a = foo, foo = _a.foo, baz = _a.baz;`.
#[test]
fn es5_var_destructuring_reassigning_rhs_uses_temp() {
    let source = "var foo: any = { foo: 1, baz: 2 };\nvar { foo, baz } = foo;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _a = foo, foo = _a.foo, baz = _a.baz;"),
        "Var declaration whose pattern reassigns the RHS identifier must capture in a temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var foo = foo.foo, baz = foo.baz;"),
        "Direct inline reads the clobbered `foo` for the second access.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_member_decorator_private_name_uses_native_static_block_scope() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var decorator: any;\nclass C1 {\n    #x;\n    @decorator((x: C1) => x.#x)\n    y() {}\n}\nclass C2 {\n    #x;\n    y(@decorator((x: C2) => x.#x) p) {}\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ESNext,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("static {\n        __decorate([\n            decorator((x) => x.#x),"),
        "Decorators that reference a private name must emit inside a class static block.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "static {\n        __decorate([\n            __param(0, decorator((x) => x.#x)),"
        ),
        "Parameter decorators that reference a private name must emit inside a class static block.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("}\n__decorate([\n    decorator((x) => x.#x),"),
        "Private-name decorator calls must not be emitted after the class body.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_member_decorator_private_name_uses_lowered_private_scope() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var decorator: any;\nclass C1 {\n    #x;\n    @decorator((x: C1) => x.#x)\n    y() {}\n}\nclass C2 {\n    #x;\n    y(@decorator((x: C2) => x.#x) p) {}\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __classPrivateFieldGet ="),
        "Lowered private-name decorator expressions must request __classPrivateFieldGet.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_C1_x = new WeakMap();\n(() => {\n    __decorate(["),
        "Lowered decorator calls must run after WeakMap initialization while private lowering state is live.\nOutput:\n{output}"
    );
    assert!(
        output.contains("decorator((x) => __classPrivateFieldGet(x, _C1_x, \"f\")),"),
        "Member decorator private access should lower through __classPrivateFieldGet.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__param(0, decorator((x) => __classPrivateFieldGet(x, _C2_x, \"f\"))),"),
        "Parameter decorator private access should lower through __classPrivateFieldGet.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("x.)"),
        "Private-name lowering must not leave an empty property access.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_decorator_trailing_comments_move_to_lowered_calls() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare function y(...args: any[]): any;\ntype T = number;\n@y(1 as T, () => C) // class decorator comment\nclass C<T> {\n    @y(null as T) // method decorator comment\n    method(@y x, y) {} // method comment\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        legacy_decorators: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("method(x, y) { } // method comment"),
        "The method's own trailing comment should remain on the method.\nOutput:\n{output}"
    );
    assert!(
        output.contains("y(null) // method decorator comment\n    ,"),
        "The erased method decorator's trailing comment should move to the lowered decorator expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("y(1, () => C) // class decorator comment"),
        "The erased class decorator's trailing comment should move to the lowered class decorator expression.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("class C {\n    //"),
        "Decorator comments must not leak into the class body after decorator tokens are erased.\nOutput:\n{output}"
    );
}

/// Regression: classes inside a namespace IIFE were missing
/// `__metadata("design:type", T)` calls under `--emitDecoratorMetadata`.
/// The namespace transformer instantiated an `ES5ClassTransformer` but
/// never forwarded the metadata flag, so decorator arrays only contained
/// the bare decorator without the type metadata.
#[test]
fn namespace_es5_class_emits_decorator_metadata() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "namespace M {\n    export function inject(t: any, k: string): void {}\n    export class Leg {}\n    export class Person {\n        @inject leftLeg: Leg;\n    }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("__metadata(\"design:type\", Leg)"),
        "Decorator metadata for the property type must emit inside the namespace IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_accessor_decorator_metadata_uses_accessor_pair_types() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var dec: any;\nclass A {\n    @dec get x() { return 0; }\n    set x(value: number) { }\n}\nclass E {\n    @dec get x() { return 0; }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __metadata ="),
        "Decorated accessors with metadata enabled must request the __metadata helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__metadata(\"design:type\", Number),\n    __metadata(\"design:paramtypes\", [Number])"
        ),
        "Accessor pairs should serialize the setter parameter type for design:type and design:paramtypes.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__metadata(\"design:type\", Object),\n    __metadata(\"design:paramtypes\", [])"
        ),
        "Getter-only accessors without an explicit type should use Object and an empty paramtypes array.\nOutput:\n{output}"
    );
}

/// Regression: ESM `--importHelpers` was not aliasing helper imports
/// when the helper name collides with a local declaration. tsc emits
/// `import { __decorate as __decorate_1 } from "tslib";` and uses
/// `__decorate_1(...)` at call sites to avoid shadowing.
#[test]
fn esm_import_helpers_aliases_when_helper_name_shadowed() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var dec: any, __decorate: any;\n@dec export class A {}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        import_helpers: true,
        legacy_decorators: true,
        emit_decorator_metadata: false,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import { __decorate as __decorate_1 } from \"tslib\";"),
        "Local `__decorate` shadowing must trigger import alias rename.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__decorate_1("),
        "Decorator call site must use the renamed alias.\nOutput:\n{output}"
    );
}

/// Counterpart: no local collision means no alias renaming.
#[test]
fn esm_import_helpers_no_alias_when_no_collision() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var dec: any;\n@dec export class A {}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        import_helpers: true,
        legacy_decorators: true,
        emit_decorator_metadata: false,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import { __decorate } from \"tslib\";"),
        "No local collision: import name should stay unaliased.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("as __decorate_1"),
        "Don't rename when there's no local shadowing.\nOutput:\n{output}"
    );
}

/// Regression: a single-line `// comment` between two class members of an
/// ES5-lowered namespace IIFE was being dropped. The trailing-standalone
/// comment extraction was skipped for class-like members on the (now
/// incorrect) assumption that the class sub-emitter would handle them, so
/// comments after the class's `}` but before the next member fell through
/// the cracks. tsc preserves them on their own line.
#[test]
fn namespace_es5_iife_preserves_line_comment_between_classes() {
    let source =
        "namespace m {\n    export class b {}\n\n    // class d\n    export class d {}\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("// class d"),
        "Single-line comment between sibling classes in a namespace IIFE must survive ES5 lowering.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_marker_strings_do_not_trigger_missing_arrow_fixture_recovery() {
    let source = r#"namespace missingCurliesWithArrow {
  const a = "namespace withStatement";
  const b = "namespace withoutStatement";
  const c = "=> var k = 10;";
  const d = "=> };";

  export const actual = 1;
}

console.log(missingCurliesWithArrow.actual);
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            module: ModuleKind::CommonJS,
            ..PrintOptions::es6()
        },
    );

    assert!(
        output.contains("missingCurliesWithArrow.actual = 1;"),
        "Valid namespace body should be emitted instead of fixture recovery output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const a = \"namespace withStatement\";"),
        "String marker declarations should remain in the namespace body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var a = () => { var k = 10; };") && !output.contains("var a = () => ;"),
        "Hardcoded missingCurliesWithArrow fixture output must not be emitted.\nOutput:\n{output}"
    );
}
