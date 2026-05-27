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
fn arrow_comments_before_token_are_erased_with_type_syntax() {
    let source = "const a = (x: string): string /* erased */ => x;\nconst b = (x: string) => /* kept */ x;\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("const a = (x) => x;"),
        "Comments before the arrow token belong to erased type syntax and should not lead the concise body.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const b = (x) => /* kept */ x;"),
        "Comments after the arrow token should still be preserved with the concise body.\nOutput:\n{output}"
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
    assert!(
        !output.contains("var C = (function ()"),
        "Variable-initializer class expression should not be wrapped in an extra IIFE.\nOutput: {output}"
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
        output.contains("C = /** @class */"),
        "Expected class expression to use assignment lhs name.\nOutput: {output}"
    );
    assert!(
        !output.contains("C = (function ()"),
        "Assignment class expression should not be wrapped in an extra IIFE.\nOutput: {output}"
    );
}

#[test]
fn test_es5_class_expression_instance_field_uses_synthetic_name() {
    let source = "const C = class { a = 1; };";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var C = /** @class */")
            && output.contains("function class_1()")
            && output.contains("this.a = 1;"),
        "Anonymous class expression with instance fields should emit as a direct IIFE with a synthetic constructor name.\nOutput: {output}"
    );
    assert!(
        !output.contains("var C = (function ()"),
        "Instance-field class expression should not be wrapped in an extra IIFE.\nOutput: {output}"
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
fn amd_known_declaration_bang_module_ignores_non_import_text() {
    let declarations = r#"declare module "loader!module" {
    export const value: number;
}
"#;
    let source = r#"/// <reference path="types.d.ts"/>

export const msg = "loader!module";
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
        "Known .d.ts files should not be preserved just because ordinary source text mentions their bang module.\nOutput:\n{output}"
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
fn static_field_class_expression_in_binding_key_uses_es5_comma_alias() {
    let source = "(({ [class { static x = 1 }.x]: b = \"\" }) => {})();";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains(
            "function (_a) {\n    var _b;\n    var _c = (_b = /** @class */ (function () {"
        ) && output.contains("function class_1()")
            && output.contains("_b.x = 1,")
            && output.contains("_b).x, _d = _a[_c], b = _d === void 0 ? \"\" : _d;"),
        "ES5 computed binding keys should reserve the class-expression alias before the key temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return _c;"),
        "Static-field class expressions in computed binding keys should not use a nested wrapper IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn nested_static_field_class_expression_uses_statement_depth_indent() {
    let source = "function outer() {\n    function inner() {\n        var y = class { static a = x };\n    }\n}";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains(
            "        var y = (_a = /** @class */ (function () {\n                function class_1() {"
        ),
        "Nested ES5 static class expression should indent the generated class IIFE by statement depth, not current visual indent width.\nOutput:\n{output}"
    );
}

#[test]
fn static_field_class_expression_in_case_body_uses_case_body_indent() {
    let source = "function f(x) {\n    switch (x) {\n        case 0:\n            var y = class { static a = x };\n    }\n}";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains(
            "            var y = (_a = /** @class */ (function () {\n                    function class_1() {"
        ),
        "Static-field class expression IIFE should indent from the case-body statement level.\nOutput:\n{output}"
    );
}

