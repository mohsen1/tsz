mod const_enums;
mod emit;
mod top_level_using;

#[cfg(test)]
mod tests {
    use crate::context::emit::EmitContext;
    use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_common::ScriptTarget;
    use tsz_parser::ParserState;

    #[test]
    fn emit_source_file_strips_top_level_blank_lines_for_js_files() {
        // tsc strips inter-statement blank lines even from JS source files.
        let source = "export const t1 = {\n    p: 'value',\n    get getter() {\n        return 'value';\n    },\n}\n\nexport const t2 = {\n    v: 'value',\n    set setter(v) {},\n}\n\nexport const t3 = {\n    p: 'value',\n    get value() {\n        return 'value';\n    },\n    set value(v) {},\n}\n";

        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("}\n\nexport const t2"),
            "JS source should NOT preserve inter-statement blank lines.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("}\n\nexport const t3"),
            "JS source should NOT preserve inter-statement blank lines.\nOutput:\n{output}"
        );
    }

    #[test]
    fn emit_source_file_does_not_preserve_top_level_blank_lines_for_ts_files() {
        let source = "export const t1 = {\n    p: 'value',\n    get getter() {\n        return 'value';\n    },\n};\n\nexport const t2 = {\n    v: 'value',\n    set setter(v) {},\n};\n\nexport const t3 = {\n    p: 'value',\n    get value() {\n        return 'value';\n    },\n    set value(v) {},\n};\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("};\n\nexport const t2"),
            "TS files should not preserve explicit inter-statement blank lines in emit.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("};\n\nexport const t3"),
            "TS files should not preserve explicit inter-statement blank lines in emit.\nOutput:\n{output}"
        );
    }

    #[test]
    fn erased_interface_member_recovery_does_not_leak_to_js() {
        let source = "interface I {\n  return (value: string): void;\n}\nconst value = 1;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                module: ModuleKind::CommonJS,
                target: ScriptTarget::ES2020,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("const value = 1;"),
            "Runtime statement should still emit.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("return (value: string): void;"),
            "Erased interface member text must not leak into JS output.\nOutput:\n{output}"
        );
    }

    #[test]
    fn ambient_module_recovery_ignores_comment_text() {
        let source = "declare module \"outer\" {\n  // module `fake` {\n  export interface Box { value: string; }\n}\nconst value = 1;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                module: ModuleKind::CommonJS,
                target: ScriptTarget::ES2020,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("const value = 1;"),
            "Runtime statement should still emit.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("declare;") && !output.contains("module `fake`;"),
            "Ambient module recovery must not scan module text from comments.\nOutput:\n{output}"
        );
    }

    #[test]
    fn for_await_temps_do_not_leak_to_source_scope() {
        let source =
            "async function f() {\n    let y: any;\n    for await (const x of y) {\n    }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let options = PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        };
        let ctx = EmitContext::with_options(options.clone());
        let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
        let mut printer =
            EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        let function_start = output.find("function f()").expect("function should emit");
        let source_scope = &output[..function_start];

        assert!(
            !source_scope.contains("var _a, e_1, _b, _c;"),
            "for-await temps should not be hoisted outside the function.\nOutput:\n{output}"
        );
        assert!(
            output.contains("function* () {\n        var _a, e_1, _b, _c;"),
            "for-await temps should be hoisted inside the generated async body.\nOutput:\n{output}"
        );
    }

    #[test]
    fn emit_class_with_accessor_members_preserves_leading_comments_in_ts_output() {
        let source = "// Regular class should still error when targeting ES5\n\
class RegularClass {\n    accessor shouldError;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        let comment_pos = output
            .find("// Regular class should still error when targeting ES5")
            .expect("accessor class comment should be emitted");
        let storage_pos = output
            .find("var _RegularClass_shouldError_accessor_storage;")
            .expect("accessor storage declaration should be emitted");
        let class_pos = output
            .find("var RegularClass =")
            .or_else(|| output.find("class RegularClass"))
            .expect("regular class declaration should be emitted");

        assert!(
            comment_pos > storage_pos,
            "Auto-accessor class leading comments should appear after storage declarations.\nOutput:\n{output}"
        );
        assert!(
            class_pos > comment_pos,
            "Class declaration should follow its auto-accessor leading comment.\nOutput:\n{output}"
        );
        assert!(
            output.contains("class RegularClass") || output.contains("var RegularClass"),
            "Class output should still be emitted for accessor-containing class in ES5 path.\nOutput:\n{output}"
        );
    }

    #[test]
    fn es5_class_duplicate_accessors_keep_first_descriptor_body() {
        let source =
            "class C {\n    get x() { return 1; }\n    get x() { return 2; } // error\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("get: function () { return 1; },"),
            "Duplicate ES5 accessor descriptor should use the first getter body.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("return 2;") && !output.contains("// error"),
            "Duplicate ES5 accessor descriptor should not inherit the later error accessor body or comment.\nOutput:\n{output}"
        );
    }

    #[test]
    fn commonjs_later_named_export_keeps_legacy_decorator_export_alias() {
        let source = "export {};\ndeclare var dec: any;\n@dec\nclass C {}\nexport { C as D };\nusing after = null;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                module: ModuleKind::CommonJS,
                legacy_decorators: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("exports.D = C;"),
            "Later named CommonJS export should preserve the pre-assignment before __decorate.\nOutput:\n{output}"
        );
        assert!(
            output.contains("exports.D = C = __decorate(["),
            "Later named CommonJS export should fuse the decorator reassignment with the export.\nOutput:\n{output}"
        );
    }

    #[test]
    fn legacy_decorated_es2015_class_self_reference_uses_hoisted_alias() {
        let source = "function decorator() { return (target: any) => {}; }\n@decorator()\nclass Foo {\n    static func(): Foo {\n        return new Foo();\n    }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                legacy_decorators: true,
                emit_decorator_metadata: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("var Foo_1;\nfunction decorator()"),
            "ES2015 decorated class self-reference alias should be hoisted before statements.\nOutput:\n{output}"
        );
        assert!(
            output.contains("let Foo = Foo_1 = class Foo"),
            "ES2015 decorated class should initialize the alias with the class expression.\nOutput:\n{output}"
        );
        assert!(
            output.contains("return new Foo_1();"),
            "ES2015 decorated class body should reference the alias.\nOutput:\n{output}"
        );
    }

    #[test]
    fn legacy_decorated_es5_class_self_reference_uses_iife_alias() {
        let source = "function decorator() { return (target: any) => {}; }\n@decorator()\nclass Foo {\n    static func(): Foo {\n        return new Foo();\n    }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                legacy_decorators: true,
                emit_decorator_metadata: true,
                target: ScriptTarget::ES5,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("Foo_1 = Foo;\n    Foo.func = function ()"),
            "ES5 decorated class should assign the alias before static members.\nOutput:\n{output}"
        );
        assert!(
            output.contains("return new Foo_1();"),
            "ES5 decorated class method should reference the alias.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var Foo_1;\n    Foo = Foo_1 = __decorate(["),
            "ES5 decorated class should declare the alias before decorating and update it from __decorate.\nOutput:\n{output}"
        );
    }

    #[test]
    fn legacy_decorated_es5_class_with_static_accessor_and_block_declares_alias_once() {
        let source = "function decorator() { return (target: any) => {}; }\n@decorator()\nclass Foo {\n    static get value() { return 1; }\n    static { Foo.value; }\n    static func(): Foo {\n        return new Foo();\n    }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                legacy_decorators: true,
                emit_decorator_metadata: true,
                target: ScriptTarget::ES5,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert_eq!(
            output.matches("var Foo_1;").count(),
            1,
            "Decorated class self-reference alias should be declared once when deferred static blocks share the static initializer queue.\nOutput:\n{output}"
        );
        assert!(
            output.contains("return new Foo_1();"),
            "Static method should still reference the decorator-stable alias.\nOutput:\n{output}"
        );
    }

    #[test]
    fn es5_object_literal_setter_downlevels_destructured_parameter() {
        let source = "const foo = {\n    set foo([start, end]: [any, any]) {\n        void start;\n        void end;\n    },\n};\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                target: ScriptTarget::ES5,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("set foo(_a) {\n        var start = _a[0], end = _a[1];"),
            "ES5 object literal setters should lower destructured parameters.\nOutput:\n{output}"
        );
    }

    #[test]
    fn decorator_metadata_conditional_type_uses_common_branch_runtime_type() {
        let source = "declare function d(): PropertyDecorator;\nabstract class BaseEntity<T> {\n    @d()\n    public attributes: T extends { attributes: infer A } ? A : undefined;\n}\nclass C {\n    @d()\n    x: number extends string ? false : true;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                legacy_decorators: true,
                emit_decorator_metadata: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("__metadata(\"design:type\", Object)\n], BaseEntity.prototype, \"attributes\", void 0);"),
            "Generic conditional metadata should stay Object.\nOutput:\n{output}"
        );
        assert!(
            output
                .contains("__metadata(\"design:type\", Boolean)\n], C.prototype, \"x\", void 0);"),
            "Conditional metadata with boolean literal branches should emit Boolean.\nOutput:\n{output}"
        );
    }

    #[test]
    fn decorator_metadata_nolib_isolated_global_type_uses_typeof_guard() {
        let source = "declare var Decorate: PropertyDecorator;\nexport class B {\n    @Decorate\n    member: Map<string, number>;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                legacy_decorators: true,
                emit_decorator_metadata: true,
                no_lib: true,
                isolated_modules: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("var _a;"),
            "Metadata guard should hoist its temp.\nOutput:\n{output}"
        );
        assert!(
            output.contains("__metadata(\"design:type\", typeof (_a = typeof Map !== \"undefined\" && Map) === \"function\" ? _a : Object)"),
            "No-lib isolated metadata should guard unresolved global constructors.\nOutput:\n{output}"
        );
    }

    #[test]
    fn commonjs_top_level_using_direct_exported_legacy_class_stays_inline() {
        let source =
            "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nexport class C {}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                module: ModuleKind::CommonJS,
                legacy_decorators: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("exports.C = C = class C {"),
            "CommonJS top-level using should keep direct legacy-decorated class exports inline.\nOutput:\n{output}"
        );
        assert!(
            output.contains("exports.C = C = __decorate(["),
            "CommonJS top-level using should preserve the exported __decorate reassignment.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("};\nexports.C = C;\n    exports.C = C = __decorate(["),
            "CommonJS top-level using should not insert a redundant trailing export between the class and __decorate.\nOutput:\n{output}"
        );
    }

    #[test]
    fn commonjs_deferred_class_export_alias_emits_after_declaration() {
        let source = "export { J as JJ };\nexport class J {}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                module: ModuleKind::CommonJS,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        let class_pos = output
            .find("class J")
            .expect("class declaration should emit");
        let direct_export_pos = output
            .find("exports.J = J;")
            .expect("direct class export should emit after the class");
        let alias_export_pos = output
            .find("exports.JJ = J;")
            .expect("deferred export alias should emit after the class");

        assert!(
            class_pos < direct_export_pos && direct_export_pos < alias_export_pos,
            "CommonJS class export aliases should be emitted after the class in tsc order.\nOutput:\n{output}"
        );
    }

    #[test]
    fn legacy_decorated_declare_computed_property_emits_decorator_target() {
        let source = "declare function decorator(target: any, key: any): any;\nconst b = Symbol('b');\nclass Foo {\n    @decorator declare [b]: number;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = EmitterPrinter::with_options(
            &parser.arena,
            PrinterOptions {
                legacy_decorators: true,
                target: ScriptTarget::ESNext,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("], Foo.prototype, b, void 0);"),
            "Legacy decorators on computed declare fields should emit the computed target expression.\nOutput:\n{output}"
        );
    }

    #[test]
    fn ambient_class_parenthesized_tail_emits_recovered_expression() {
        let source = "declare class foo();\nfunction foo() {}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
            output.starts_with("\"use strict\";\n();\nfunction foo() { }"),
            "Malformed ambient class tail should emit the recovered `();` expression.\nOutput:\n{output}"
        );
    }

    #[test]
    fn invalid_var_class_keyword_emits_recovered_class_tail() {
        let source = "var export;\nvar foo;\nvar class;\nvar bar;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
            output.contains("var ;\nclass {\n}\n;\nvar bar;"),
            "`var class;` should emit tsc's recovered anonymous class tail.\nOutput:\n{output}"
        );
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
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
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
    fn esm_emits_export_empty_when_only_type_exports() {
        // When a file's only module syntax is `export {};`, it should be preserved
        // to maintain ESM semantics.
        let source = "export {};\nconst x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
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

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
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
}
