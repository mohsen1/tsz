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
fn trailing_decimal_numeric_follow_recovery_keeps_call_tail() {
    let source = "var test = 2.toString();\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("var test = 2., toString;\n();"),
        "Numeric-follow recovery should preserve the identifier and call tail like tsc.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var test = 2.;"),
        "Recovered numeric-follow initializer must not drop the identifier and call tail.\nOutput:\n{output}"
    );
}

#[test]
fn trailing_decimal_access_spacing_still_disambiguates_property_dot() {
    let source = "var test3 = 3 .toString();\nvar test11 = 3. /* comment */ .toString();\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("var test3 = 3..toString();"),
        "Integer literal property access separated by whitespace still needs a double dot.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var test11 = 3. /* comment */.toString();"),
        "Trailing-decimal literals with preserved comments should keep one property dot.\nOutput:\n{output}"
    );
}

#[test]
fn trailing_decimal_remove_comments_keeps_newline_separator() {
    let source = "var test15 = 3.\n    // comment\n    .toString();\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            remove_comments: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var test15 = 3.\n    .toString();"),
        "Removing comments should preserve the newline after a trailing-decimal literal before property access.\nOutput:\n{output}"
    );
}

#[test]
fn system_reexported_namespace_folds_export_into_es2015_iife_tail() {
    let source = "namespace N { export const x = 1; }\nexport { N as Out };\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::System,
            ..Default::default()
        },
    );

    assert!(
        output.contains(r#"})(N || (exports_1("Out", N = {})));"#),
        "System namespace re-export should be scheduled in the IIFE tail.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(r#"exports_1("Out", N);"#),
        "System namespace re-export should not emit a redundant separate export call.\nOutput:\n{output}"
    );
}

#[test]
fn system_reexported_namespace_folds_export_into_es5_iife_tail() {
    let source = "namespace N { export var x = 1; }\nexport { N as Out };\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            module: ModuleKind::System,
            ..Default::default()
        },
    );

    assert!(
        output.contains(r#"})(N || (exports_1("Out", N = {})));"#),
        "System ES5 namespace re-export should be scheduled in the IR IIFE tail.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(r#"exports_1("Out", N);"#),
        "System ES5 namespace re-export should not rely on a separate export call.\nOutput:\n{output}"
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
fn test_invalid_predefined_interface_names_recover_tsc_runtime_tokens() {
    let source =
        "interface any { }\ninterface string { }\ninterface void {}\ninterface number<T> {}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("interface;\n"),
        "Invalid `interface any` should recover the runtime interface token statement.\nOutput:\n{output}"
    );
    assert!(
        output.contains("void {};"),
        "Invalid `interface void` should recover as a void object statement.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("interface string") && !output.contains("interface number"),
        "Other predefined type-name interfaces should stay erased here.\nOutput:\n{output}"
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
fn merged_enum_forward_references_to_later_block_emit_zero() {
    let source = r#"enum E {
    A = B,
    A1 = E["B"],
    B = 1,
    C = E.D,
    C1 = E["D"]
}

enum E {
    D = 4
}"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains(r#"E[E["A"] = 0] = "A";"#),
        "Bare forward refs in the same enum block should emit 0.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"E[E["A1"] = 0] = "A1";"#),
        "Element forward refs in the same enum block should emit 0.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"E[E["C"] = 0] = "C";"#),
        "Property refs to later merged enum blocks should emit 0.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"E[E["C1"] = 0] = "C1";"#),
        "Element refs to later merged enum blocks should emit 0.\nOutput:\n{output}"
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
fn invalid_namespace_static_var_and_function_modifiers_are_preserved() {
    let source = r#"namespace N {
    public var publicValue: number = 0;
    static var staticValue: number = 1;
    private function privateFn(x: string) { }
    static function staticFn(x: string) { }
}"#;
    let output = parse_lower_print(source, PrintOptions::default());

    assert!(
        output.contains("var publicValue = 0;"),
        "Invalid access modifier on namespace var should be erased.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static var staticValue = 1;"),
        "Invalid static modifier on namespace var should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function privateFn(x) { }"),
        "Invalid access modifier on namespace function should be erased.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static function staticFn(x) { }"),
        "Invalid static modifier on namespace function should be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_namespace_static_class_enum_and_namespace_modifiers_are_preserved() {
    let source = r#"namespace N {
    public class PublicClass { }
    private class PrivateClass { }
    static class StaticClass { }
    static namespace Inner { export var value = 1; }
    static enum Color { Red }
}"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("class PublicClass"),
        "Invalid public modifier on namespace class should be erased.\nOutput:\n{output}"
    );
    assert!(
        output.contains("class PrivateClass"),
        "Invalid private modifier on namespace class should be erased.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("public class") && !output.contains("private class"),
        "Access modifiers on namespace classes must not survive JS emit.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static class StaticClass"),
        "Invalid static modifier on namespace class should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static let Inner;"),
        "Invalid static modifier on nested namespace binding should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static let Color;"),
        "Invalid static modifier on namespace enum binding should be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_namespace_static_modifiers_are_erased_for_es5() {
    let source = r#"namespace N {
    static var staticValue: number = 1;
    static function staticFn(x: string) { }
    static class StaticClass { }
    static namespace Inner { export var value = 1; }
    static enum Color { Red }
}"#;
    let output = parse_lower_print(source, PrintOptions::es5());

    assert!(
        output.contains("var staticValue = 1;"),
        "ES5 namespace var recovery should erase invalid static.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function staticFn(x) { }"),
        "ES5 namespace function recovery should erase invalid static.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var Inner;"),
        "ES5 namespace recovery should still emit the nested namespace binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var Color;"),
        "ES5 namespace recovery should still emit the enum binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("static "),
        "ES5 namespace output must not preserve invalid static modifiers.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_namespace_static_async_function_modifier_is_preserved_before_lowering() {
    let source = r#"namespace N {
    static async function staticAsync() { }
    static async function* staticAsyncGen() { }
}"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("static function staticAsync()"),
        "Invalid static modifier should be preserved on lowered async namespace functions.\nOutput:\n{output}"
    );
    assert!(
        output.contains("static function staticAsyncGen()"),
        "Invalid static modifier should be preserved on lowered async generator namespace functions.\nOutput:\n{output}"
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

