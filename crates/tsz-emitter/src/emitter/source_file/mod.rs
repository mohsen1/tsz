mod const_enums;
mod emit;
mod top_level_using;

#[cfg(test)]
mod tests {
    use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
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
