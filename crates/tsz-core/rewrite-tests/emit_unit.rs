use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bind::bind_source_with_kind;
use crate::config::ProjectProvenance;
use crate::emit_paths::EmitPlan;
use crate::program::{
    CapabilityAnalysis, CapabilityContext, CapabilityScope, CapabilityTarget, Compiler,
    CompilerOptions, EmittedFile, ProgramFile, SourceInput,
};
use crate::source::{FileId, SourceText};
use crate::syntax::{StatementKind, parse_source};

use super::{Printer, emit_file_with_plan};

pub(super) fn emit_file(file: &ProgramFile, options: &CompilerOptions) -> Vec<EmittedFile> {
    let capabilities = CapabilityAnalysis::derive(
        std::slice::from_ref(file),
        options,
        CapabilityContext::default(),
    );
    let plan = EmitPlan::for_program(
        std::slice::from_ref(file),
        options,
        &ProjectProvenance::default(),
        &capabilities,
    );
    emit_file_with_plan(
        file,
        options,
        plan.for_file(file.source.id),
        &Default::default(),
    )
}

fn program_file(path: &str, text: &str) -> ProgramFile {
    let source = SourceText::new(FileId(0), PathBuf::from(path), Arc::<str>::from(text));
    let parsed = parse_source(&source);
    assert!(
        parsed.diagnostics.is_empty(),
        "test source must parse without diagnostics: {:?}",
        parsed.diagnostics
    );
    let bindings = bind_source_with_kind(
        source.id,
        crate::source::SourceKind::TypeScript,
        &parsed.unit,
    );
    ProgramFile {
        source,
        syntax: parsed.unit,
        bindings,
    }
}

#[test]
fn conditional_expression_printer_preserves_branch_association() {
    let source = concat!(
        "const nested = flag ? 1 : other ? 2 : 3;\n",
        "const grouped = (flag ? 1 : 2) ? 3 : 4;\n",
    );
    let file = program_file("conditional.ts", source);
    let options = CompilerOptions {
        target: "es2022".to_string(),
        ..CompilerOptions::default()
    };
    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(javascript.finish(), format!("\"use strict\";\n{source}"));
}

#[test]
fn authored_strict_directive_controls_the_synthesized_prologue() {
    for (name, module, source, expected) in [
        (
            "script-plain",
            "",
            "// keep leading\n\"use strict\";\n0;\n",
            "// keep leading\n\"use strict\";\n0;\n",
        ),
        (
            "script-directive-comments",
            "",
            "\"use asm\";\n/* keep between */\n'use strict';\n0;\n",
            "\"use strict\";\n\"use asm\";\n/* keep between */\n'use strict';\n0;\n",
        ),
        (
            "script-second-position-strict",
            "",
            "\"use asm\";\n\"use strict\";\n0;\n",
            "\"use strict\";\n\"use asm\";\n\"use strict\";\n0;\n",
        ),
        (
            "script-other-directive",
            "",
            "\"use asm\";\n0;\n",
            "\"use strict\";\n\"use asm\";\n0;\n",
        ),
        (
            "strict-after-non-directive",
            "",
            "0;\n\"use strict\";\n",
            "\"use strict\";\n0;\n\"use strict\";\n",
        ),
        (
            "erased-prefix",
            "",
            concat!(
                "interface Gone {}\n",
                "declare const erased: number;\n",
                "type Hidden = string;\n",
                "\"use strict\";\n",
                "0;\n",
            ),
            "\"use strict\";\n\"use strict\";\n0;\n",
        ),
        (
            "fixed-escape",
            "",
            "\"use\\x20strict\";\n0;\n",
            "\"use\\x20strict\";\n0;\n",
        ),
        (
            "extended-escape",
            "",
            "\"use\\u{20}strict\";\n0;\n",
            "\"use\\u{20}strict\";\n0;\n",
        ),
        (
            "extended-strict-in-second-position",
            "",
            "\"use asm\";\n\"use\\u{20}strict\";\n0;\n",
            "\"use strict\";\n\"use asm\";\n\"use\\u{20}strict\";\n0;\n",
        ),
        (
            "extended-non-strict-stops-prefix",
            "",
            "\"use\\u{21}strict\";\n\"use strict\";\n0;\n",
            "\"use strict\";\n\"use\\u{21}strict\";\n\"use strict\";\n0;\n",
        ),
        (
            "parenthesized-non-directive",
            "",
            "(\"use strict\");\n0;\n",
            "\"use strict\";\n(\"use strict\");\n0;\n",
        ),
        (
            "commonjs-erased-prefix",
            "commonjs",
            concat!(
                "import type { Shape } from \"./types\";\n",
                "interface Gone {}\n",
                "declare const erased: number;\n",
                "\"use strict\";\n",
                "export const value = 1;\n",
            ),
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.value = void 0;\n",
                "\"use strict\";\n",
                "exports.value = 1;\n",
            ),
        ),
        (
            "commonjs-type-only-import-prefix",
            "commonjs",
            concat!(
                "import type { Shape } from \"./types\";\n",
                "\"use strict\";\n",
                "export const value = 1;\n",
            ),
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.value = void 0;\n",
                "exports.value = 1;\n",
            ),
        ),
        (
            "commonjs-all-type-binding-import-prefix",
            "commonjs",
            concat!(
                "import { type Shape } from \"./types\";\n",
                "\"use strict\";\n",
                "export const value = 1;\n",
            ),
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.value = void 0;\n",
                "exports.value = 1;\n",
            ),
        ),
        (
            "commonjs-type-only-export-prefix",
            "commonjs",
            concat!(
                "export type { Shape };\n",
                "\"use strict\";\n",
                "export const value = 1;\n",
            ),
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.value = void 0;\n",
                "exports.value = 1;\n",
            ),
        ),
        (
            "commonjs-type-only-export-all-prefix",
            "commonjs",
            concat!(
                "export type * from \"./types\";\n",
                "\"use strict\";\n",
                "export const value = 1;\n",
            ),
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.value = void 0;\n",
                "exports.value = 1;\n",
            ),
        ),
        (
            "commonjs-empty-export-from-prefix",
            "commonjs",
            concat!(
                "export {} from \"./types\";\n",
                "\"use strict\";\n",
                "export const value = 1;\n",
            ),
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.value = void 0;\n",
                "exports.value = 1;\n",
            ),
        ),
        (
            "exported-type-alias-stops-prefix",
            "commonjs",
            concat!(
                "export type Shape = string;\n",
                "\"use strict\";\n",
                "export const value = 1;\n",
            ),
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.value = void 0;\n",
                "\"use strict\";\n",
                "exports.value = 1;\n",
            ),
        ),
        (
            "overload-stops-prefix",
            "",
            "function erased(): void;\n\"use strict\";\n0;\n",
            "\"use strict\";\n\"use strict\";\n0;\n",
        ),
        (
            "declare-stops-prefix",
            "",
            "declare const erased: number;\n\"use strict\";\n0;\n",
            "\"use strict\";\n\"use strict\";\n0;\n",
        ),
        (
            "commonjs-comment-boundary",
            "commonjs",
            concat!(
                "\"use asm\";\n",
                "/* keep between */\n",
                "\"use strict\";\n",
                "/* keep after */\n",
                "export const value = 1;\n",
            ),
            concat!(
                "\"use strict\";\n",
                "\"use asm\";\n",
                "/* keep between */\n",
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.value = void 0;\n",
                "/* keep after */\n",
                "exports.value = 1;\n",
            ),
        ),
        (
            "commonjs-script-without-helper",
            "commonjs",
            "\"use asm\";\n0;\n",
            "\"use strict\";\n\"use asm\";\n0;\n",
        ),
        (
            "esm-external-without-synthetic-strict",
            "esnext",
            "\"use asm\";\nexport const value = 1;\n",
            "\"use asm\";\nexport const value = 1;\n",
        ),
    ] {
        let file = program_file("case.ts", source);
        let options = CompilerOptions {
            module: module.to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        };
        let mut javascript = Printer::new(&file.source, &file.bindings, &options);
        javascript.emit_javascript(&file.syntax);
        assert_eq!(javascript.finish(), expected, "{name}");
    }
}

#[test]
fn empty_braced_bodies_preserve_authored_line_shape() {
    let options = CompilerOptions {
        target: "es2022".to_string(),
        ..CompilerOptions::default()
    };
    for (name, source, expected) in [
        (
            "lf",
            "function lf() {\n}\n",
            "\"use strict\";\nfunction lf() {\n}\n",
        ),
        (
            "crlf-canonicalizes",
            "function crlf() {\r\n}\r\n",
            "\"use strict\";\nfunction crlf() {\n}\n",
        ),
        (
            "cr-canonicalizes",
            "function cr() {\r}\r",
            "\"use strict\";\nfunction cr() {\n}\n",
        ),
        (
            "line-separator-canonicalizes",
            "function line_separator() {\u{2028}}\n",
            "\"use strict\";\nfunction line_separator() {\n}\n",
        ),
        (
            "paragraph-separator-canonicalizes",
            "function paragraph_separator() {\u{2029}}\n",
            "\"use strict\";\nfunction paragraph_separator() {\n}\n",
        ),
        (
            "same-line",
            "function same() {}\n",
            "\"use strict\";\nfunction same() { }\n",
        ),
        (
            "class-hosts",
            "class C {\nconstructor() {\n}\nmethod() {\n}\n}\n",
            concat!(
                "\"use strict\";\n",
                "class C {\n",
                "    constructor() {\n",
                "    }\n",
                "    method() {\n",
                "    }\n",
                "}\n",
            ),
        ),
        (
            "expression-hosts",
            concat!(
                "const expression = function () {\n};\n",
                "const arrow = () => {\n};\n",
                "const object = { method() {\n} };\n",
            ),
            concat!(
                "\"use strict\";\n",
                "const expression = function () {\n};\n",
                "const arrow = () => {\n};\n",
                "const object = { method() {\n} };\n",
            ),
        ),
        (
            "statement-hosts",
            "{\n}\nif (true) {\n}\n",
            "\"use strict\";\n{\n}\nif (true) {\n}\n",
        ),
    ] {
        let file = program_file("case.ts", source);
        let mut javascript = Printer::new(&file.source, &file.bindings, &options);
        javascript.emit_javascript(&file.syntax);
        assert_eq!(javascript.finish(), expected, "{name}");
    }

    let mut missing_span = program_file("case.ts", "function recovered() {\n}\n");
    let StatementKind::Function(declaration) = &mut missing_span.syntax.statements[0].kind else {
        unreachable!()
    };
    declaration.body_span = None;
    let output = emit_file(&missing_span, &options);
    assert_eq!(
        output[0].text,
        "\"use strict\";\nfunction recovered() { }\n"
    );
}

#[test]
fn empty_multiline_body_comments_respect_remove_comments() {
    for (name, remove_comments, expected) in [
        (
            "retained",
            false,
            "\"use strict\";\nfunction commented() {\n    /* retained */\n}\n",
        ),
        (
            "removed",
            true,
            "\"use strict\";\nfunction commented() {\n}\n",
        ),
    ] {
        let options = CompilerOptions {
            remove_comments,
            ..CompilerOptions::default()
        };
        let output = emit_file(
            &program_file("case.ts", "function commented() {\n    /* retained */\n}\n"),
            &options,
        );
        assert_eq!(output[0].text, expected, "{name}");
    }
}

#[test]
fn explicit_redundant_module_aliases_survive_in_both_printers() {
    let source = concat!(
        "import { \\u0061 as a, b as \\u0062 } from \"./m\";\n",
        "export { \\u0061 as a, b as \\u0062 };\n",
    );
    let file = program_file("case.ts", source);
    let options = CompilerOptions {
        target: "es2025".to_string(),
        module: "preserve".to_string(),
        ..CompilerOptions::default()
    };

    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(javascript.finish(), source);

    let mut declaration = Printer::new(&file.source, &file.bindings, &options);
    declaration.emit_declarations(
        &file.syntax,
        &super::reachability::DeclarationReachability::All,
    );
    assert_eq!(declaration.finish(), source);
}

#[test]
fn commonjs_import_export_and_variable_identifier_spelling_is_exact() {
    let source = concat!(
        "import { \\u0061 as local\\u0062, c as \\u0064 } from \"./m\";\n",
        "export { local\\u0062 as \\u0065, \\u0064 as f };\n",
        "export const a\\u0062 = 1;\n",
    );
    let file = program_file("case.ts", source);
    let options = CompilerOptions {
        target: "es2025".to_string(),
        module: "commonjs".to_string(),
        ..CompilerOptions::default()
    };

    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            "exports.ab = void 0;\n",
            "const localb = require(\"./m\").a;\n",
            "const d = require(\"./m\").c;\n",
            "Object.defineProperty(exports, \"\\\\u0065\", { enumerable: true, get: function () { return localb; } });\n",
            "Object.defineProperty(exports, \"f\", { enumerable: true, get: function () { return d; } });\n",
            "exports.a\\u0062 = 1;\n",
        )
    );
}

#[test]
fn commonjs_exported_variable_uses_module_slot_and_preserves_detached_header() {
    let source = concat!(
        "// repro from an upstream issue\n",
        "\n",
        "export let cedar = [{ leaf: 0, grow() {} }, { branch: 1 }];\n",
    );
    let file = program_file("case.ts", source);
    let options = CompilerOptions {
        target: "es2015".to_string(),
        module: "commonjs".to_string(),
        ..CompilerOptions::default()
    };

    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "// repro from an upstream issue\n",
            "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            "exports.cedar = void 0;\n",
            "exports.cedar = [{ leaf: 0, grow() { } }, { branch: 1 }];\n",
        )
    );
}

#[test]
fn commonjs_exported_variable_references_follow_binding_identity() {
    let source = concat!(
        "export let cedar = 1;\n",
        "export function bump() { cedar += 1; return { cedar }; }\n",
        "export function shadow(cedar: number) { cedar += 1; return { cedar }; }\n",
    );
    let file = program_file("case.ts", source);
    let options = CompilerOptions {
        target: "es2015".to_string(),
        module: "commonjs".to_string(),
        ..CompilerOptions::default()
    };

    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            "exports.cedar = void 0;\n",
            "exports.bump = bump;\n",
            "exports.shadow = shadow;\n",
            "exports.cedar = 1;\n",
            "function bump() { exports.cedar += 1; return { cedar: exports.cedar }; }\n",
            "function shadow(cedar) { cedar += 1; return { cedar }; }\n",
        )
    );
}

#[test]
fn ambient_exports_never_reenter_javascript_module_lowering() {
    let mixed = concat!(
        "export declare const amb\\u0069ent: number;\n",
        "export let r\\u0075ntime = 1;\n",
        "r\\u0075ntime += amb\\u0069ent;\n",
    );
    for (name, module, source, expected) in [
        (
            "commonjs-ambient-only",
            "commonjs",
            "export declare const erased: number;\n",
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            ),
        ),
        (
            "commonjs-mixed-escaped",
            "commonjs",
            mixed,
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.runtime = void 0;\n",
                "exports.r\\u0075ntime = 1;\n",
                "exports.r\\u0075ntime += amb\\u0069ent;\n",
            ),
        ),
        (
            "commonjs-all-ambient-declaration-kinds",
            "commonjs",
            concat!(
                "export declare function erasedFunction(): void;\n",
                "export declare class ErasedClass {}\n",
                "declare const localAmbient: number;\n",
                "export const kept = localAmbient;\n",
            ),
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.kept = void 0;\n",
                "exports.kept = localAmbient;\n",
            ),
        ),
        (
            "esmodule-ambient-only",
            "esnext",
            "export declare const erased: number;\n",
            "export {};\n",
        ),
        (
            "esmodule-mixed-escaped",
            "esnext",
            mixed,
            concat!(
                "export let r\\u0075ntime = 1;\n",
                "r\\u0075ntime += amb\\u0069ent;\n",
            ),
        ),
    ] {
        let file = program_file("case.ts", source);
        let options = CompilerOptions {
            target: "es2025".to_string(),
            module: module.to_string(),
            ..CompilerOptions::default()
        };
        let mut javascript = Printer::new(&file.source, &file.bindings, &options);
        javascript.emit_javascript(&file.syntax);
        assert_eq!(javascript.finish(), expected, "{name}");
    }
}

#[test]
fn commonjs_generated_function_and_class_names_are_cooked() {
    let source = concat!(
        "export function f\\u0063() { return 1; }\n",
        "export class C\\u0064 {}\n",
    );
    let file = program_file("case.ts", source);
    let options = CompilerOptions {
        target: "es2025".to_string(),
        module: "commonjs".to_string(),
        ..CompilerOptions::default()
    };

    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            "exports.Cd = void 0;\n",
            "exports.fc = fc;\n",
            "function fc() { return 1; }\n",
            "class Cd {\n",
            "}\n",
            "exports.Cd = Cd;\n",
        )
    );
}

#[test]
fn commonjs_named_declaration_prologue_preserves_root_and_comment_order() {
    let source = concat!(
        "0;\n",
        "/* before first */\n",
        "export class F\\u0069rst {\n",
        "}\n",
        "/* before alpha */\n",
        "export function al\\u0070ha() {\n",
        "    return 1;\n",
        "}\n",
        "/* before second */\n",
        "export class Second {\n",
        "}\n",
        "/* before beta */\n",
        "export function beta() {\n",
        "    return 2;\n",
        "}\n",
    );
    let file = program_file("case.ts", source);
    let options = CompilerOptions {
        target: "es2025".to_string(),
        module: "commonjs".to_string(),
        ..CompilerOptions::default()
    };

    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            "exports.Second = exports.First = void 0;\n",
            "exports.alpha = alpha;\n",
            "exports.beta = beta;\n",
            "0;\n",
            "/* before first */\n",
            "class First {\n",
            "}\n",
            "exports.First = First;\n",
            "/* before alpha */\n",
            "function alpha() {\n",
            "    return 1;\n",
            "}\n",
            "/* before second */\n",
            "class Second {\n",
            "}\n",
            "exports.Second = Second;\n",
            "/* before beta */\n",
            "function beta() {\n",
            "    return 2;\n",
            "}\n",
        )
    );
}

#[test]
fn named_declaration_prologue_is_commonjs_only() {
    let source = concat!(
        "export function f\\u0063(): number {\n",
        "    return 1;\n",
        "}\n",
        "export class C\\u0064 {\n",
        "}\n",
    );
    let expected = concat!(
        "export function f\\u0063() {\n",
        "    return 1;\n",
        "}\n",
        "export class C\\u0064 {\n",
        "}\n",
    );

    for module in ["esnext", "preserve"] {
        let file = program_file("case.ts", source);
        let options = CompilerOptions {
            target: "es2025".to_string(),
            module: module.to_string(),
            ..CompilerOptions::default()
        };
        let mut javascript = Printer::new(&file.source, &file.bindings, &options);
        javascript.emit_javascript(&file.syntax);
        assert_eq!(javascript.finish(), expected, "{module}");
    }
}

#[test]
fn commonjs_local_export_specifier_uses_cooked_assignment_names() {
    let source = concat!(
        "const local\\u0062 = 1;\n",
        "export { local\\u0062 as \\u0065 };\n",
    );
    let file = program_file("case.ts", source);
    let options = CompilerOptions {
        target: "es2025".to_string(),
        module: "commonjs".to_string(),
        ..CompilerOptions::default()
    };

    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            "const local\\u0062 = 1;\n",
            "exports.e = localb;\n",
        )
    );
}

#[test]
fn commonjs_direct_module_reexport_cooks_key_but_preserves_source_property() {
    let source = "export { \\u0061 as \\u0065 } from \"./m\";\n";
    let file = program_file("case.ts", source);
    let options = CompilerOptions {
        target: "es2025".to_string(),
        module: "commonjs".to_string(),
        ..CompilerOptions::default()
    };

    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            "Object.defineProperty(exports, \"e\", { enumerable: true, get: function () { return require(\"./m\").\\u0061; } });\n",
        )
    );
}

#[test]
fn erases_type_only_syntax_and_annotations() {
    let file = program_file(
        "input.ts",
        concat!(
            "interface Point { x: number; }\n",
            "type Scalar = number;\n",
            "const point: Point = { x: 1 };\n",
            "function add(a: number, b: number): number { return a + b; }\n",
        ),
    );
    let options = CompilerOptions {
        target: "esnext".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };

    let output = emit_file(&file, &options);
    assert_eq!(output.len(), 1);
    assert_eq!(
        output[0].text,
        concat!(
            "\"use strict\";\n",
            "const point = { x: 1 };\n",
            "function add(a, b) { return a + b; }\n",
        )
    );
}

#[test]
fn emits_written_declaration_shapes_without_checking() {
    let file = program_file(
        "src/api.ts",
        concat!(
            "export const greeting: string = \"hello\";\n",
            "export interface Box<T> { readonly value?: T; }\n",
            "export function id<T>(value: T): T { return value; }\n",
        ),
    );
    let options = CompilerOptions {
        declaration: true,
        target: "esnext".to_string(),
        module: "esnext".to_string(),
        out_dir: Some(PathBuf::from("dist")),
        declaration_dir: Some(PathBuf::from("types")),
        ..CompilerOptions::default()
    };

    let output = emit_file(&file, &options);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0].path, Path::new("dist/api.js"));
    assert_eq!(output[1].path, Path::new("types/api.d.ts"));
    assert_eq!(
        output[0].text,
        concat!(
            "export const greeting = \"hello\";\n",
            "export function id(value) { return value; }\n",
        )
    );
    assert_eq!(
        output[1].text,
        concat!(
            "export declare const greeting: string;\n",
            "export interface Box<T> {\n",
            "    readonly value?: T;\n",
            "}\n",
            "export declare function id<T>(value: T): T;\n",
        )
    );
}

#[test]
fn preserves_es_modules_at_the_ts7_defaults() {
    let file = program_file("value.ts", "export const value: number = 1;\n");
    let output = emit_file(&file, &CompilerOptions::default());
    assert_eq!(output[0].text, "export const value = 1;\n");
}

#[test]
fn keeps_expression_grouping_while_erasing_assertions() {
    let file = program_file(
        "math.ts",
        "const result: number = (1 + 2) * (3 as number);\n",
    );
    let options = CompilerOptions {
        target: "esnext".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        "\"use strict\";\nconst result = (1 + 2) * 3;\n"
    );
}

#[test]
fn new_callee_parentheses_follow_assertion_erased_precedence() {
    let file = program_file(
        "new-assertions.ts",
        concat!(
            "declare const C: any; declare const factory: any; ",
            "declare const namespaceValue: any;\n",
            "new (factory() as any);\n",
            "new ((new C()) as any);\n",
            "new (C as any)();\n",
            "new (namespaceValue.Member as any)();\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2015".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        concat!(
            "\"use strict\";\n",
            "new (factory());\n",
            "new (new C());\n",
            "new C();\n",
            "new namespaceValue.Member();\n",
        )
    );
}

#[test]
fn derives_module_extension_outputs_without_losing_module_identity() {
    let file = program_file("src/value.mts", "export const value: number = 1;\n");
    let options = CompilerOptions {
        declaration: true,
        target: "esnext".to_string(),
        module: "nodenext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(output[0].path, Path::new("src/value.mjs"));
    assert_eq!(output[1].path, Path::new("src/value.d.mts"));
    assert_eq!(output[0].text, "export const value = 1;\n");
    assert_eq!(output[1].text, "export declare const value: number;\n");
}

#[test]
fn emits_modern_module_classes_from_structured_nodes() {
    let file = program_file(
        "src/service.ts",
        concat!(
            "import { token } from \"./token\";\n",
            "export class Service extends Base {\n",
            "  value: number = token;\n",
            "  constructor(value: number) { this.value = value; }\n",
            "  static create(value: number): Service { return new Service(value); }\n",
            "}\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2022".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        concat!(
            "import { token } from \"./token\";\n",
            "export class Service extends Base {\n",
            "    value = token;\n",
            "    constructor(value) { this.value = value; }\n",
            "    static create(value) { return new Service(value); }\n",
            "}\n",
        )
    );
}

#[test]
fn erases_class_overload_signatures_but_keeps_empty_implementations() {
    let file = program_file(
        "service.ts",
        concat!(
            "class Service {\n",
            "  constructor(value: string);\n",
            "  constructor() {}\n",
            "  method(value: string): void;\n",
            "  method() {}\n",
            "}\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2022".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        concat!(
            "\"use strict\";\n",
            "class Service {\n",
            "    constructor() { }\n",
            "    method() { }\n",
            "}\n",
        )
    );
}

#[test]
fn authored_empty_class_elements_survive_javascript_but_not_declarations() {
    let source = concat!(
        "export class Empty { ;;; }\n",
        "export class Around {\n",
        "  ; // after first\n",
        "  field: number = 1;\n",
        "  ; /* between */ ;\n",
        "  method() {}\n",
        "  ;\n",
        "}\n",
        "export class Control { field: number = 1; method() {} }\n",
    );
    let file = program_file("empty-elements.ts", source);
    let classes = file
        .syntax
        .statements
        .iter()
        .filter_map(|statement| match &statement.kind {
            StatementKind::Class(declaration) => Some(declaration),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(classes.len(), 3);
    assert_eq!(classes[0].empty_elements.len(), 3);
    assert_eq!(classes[1].empty_elements.len(), 4);
    assert!(classes[2].empty_elements.is_empty());

    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "empty-elements.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            declaration: true,
            target: "es2022".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{output:#?}");
    assert_eq!(
        output.semantic_completion,
        crate::program::SemanticCompletion::Complete
    );
    let product = |path: &str| {
        output
            .emitted_files
            .iter()
            .find(|file| file.path == Path::new(path))
            .map(|file| file.text.as_str())
    };
    assert_eq!(
        product("empty-elements.js"),
        Some(concat!(
            "export class Empty {\n",
            "    ;\n",
            "    ;\n",
            "    ;\n",
            "}\n",
            "export class Around {\n",
            "    ; // after first\n",
            "    field = 1;\n",
            "    ; /* between */\n",
            "    ;\n",
            "    method() { }\n",
            "    ;\n",
            "}\n",
            "export class Control {\n",
            "    field = 1;\n",
            "    method() { }\n",
            "}\n",
        )),
    );
    assert_eq!(
        product("empty-elements.d.ts"),
        Some(concat!(
            "export declare class Empty {\n",
            "}\n",
            "export declare class Around {\n",
            "    field: number;\n",
            "    method(): void;\n",
            "}\n",
            "export declare class Control {\n",
            "    field: number;\n",
            "    method(): void;\n",
            "}\n",
        )),
    );
}

#[test]
fn rewrites_mixed_esm_clauses_and_assertions_from_structured_nodes() {
    let file = program_file(
        "module.ts",
        concat!(
            "import Default, { type Shape, live as renamed } from \"./dep\";\n",
            "export { type Shape, renamed as exposed } from \"./dep\";\n",
            "const value = [Default, renamed] as unknown;\n",
            "export default (value as unknown);\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2022".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        concat!(
            "import Default, { live as renamed } from \"./dep\";\n",
            "export { renamed as exposed } from \"./dep\";\n",
            "const value = [Default, renamed];\n",
            "export default value;\n",
        )
    );
}

#[test]
fn emits_supported_class_fields_and_preserves_private_names() {
    let file = program_file(
        "model.ts",
        concat!(
            "class Model {\n",
            "  #secret = 1;\n",
            "  visible: number;\n",
            "}\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2022".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let output = emit_file(&file, &options);
    assert_eq!(
        output[0].text,
        concat!(
            "\"use strict\";\n",
            "class Model {\n",
            "    #secret = 1;\n",
            "    visible;\n",
            "}\n",
        )
    );
}

#[test]
fn default_export_declarations_use_the_default_commonjs_key_and_exact_dts_spelling() {
    let cases = [
        (
            "named-class.ts",
            "export default class Named {}\n",
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "class Named {\n",
                "}\n",
                "exports.default = Named;\n",
            ),
            "export default class Named {\n}\n",
        ),
        (
            "escaped-class.ts",
            "export default class C\\u006cassed {}\n",
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "class Classed {\n",
                "}\n",
                "exports.default = Classed;\n",
            ),
            "export default class C\\u006cassed {\n}\n",
        ),
        (
            "anonymous-class.ts",
            "export default class {}\n",
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "class default_1 {\n",
                "}\n",
                "exports.default = default_1;\n",
            ),
            "export default class {\n}\n",
        ),
        (
            "anonymous-class-extends.ts",
            "class Base {}\nexport default class extends Base {}\n",
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "class Base {\n",
                "}\n",
                "class default_1 extends Base {\n",
                "}\n",
                "exports.default = default_1;\n",
            ),
            concat!(
                "declare class Base {\n",
                "}\n",
                "export default class extends Base {\n",
                "}\n",
            ),
        ),
        (
            "named-function.ts",
            "0;\nexport default function named(): number { return 1; }\n",
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.default = named;\n",
                "0;\n",
                "function named() { return 1; }\n",
            ),
            "export default function named(): number;\n",
        ),
        (
            "escaped-function.ts",
            "0;\nexport default function f\\u0075n(): number { return 1; }\n",
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.default = fun;\n",
                "0;\n",
                "function fun() { return 1; }\n",
            ),
            "export default function f\\u0075n(): number;\n",
        ),
        (
            "anonymous-function.ts",
            "export default function (): number { return 1; }\n",
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "exports.default = default_1;\n",
                "function default_1() { return 1; }\n",
            ),
            "export default function (): number;\n",
        ),
    ];

    for (path, source, expected_javascript, expected_declaration) in cases {
        let file = program_file(path, source);
        let options = CompilerOptions {
            declaration: true,
            target: "es2022".to_string(),
            module: "commonjs".to_string(),
            ..CompilerOptions::default()
        };
        let mut javascript = Printer::new(&file.source, &file.bindings, &options);
        javascript.emit_javascript(&file.syntax);
        assert_eq!(
            javascript.finish(),
            expected_javascript,
            "{path} JavaScript"
        );
        let mut declaration = Printer::new(&file.source, &file.bindings, &options);
        declaration.emit_declarations(
            &file.syntax,
            &super::reachability::DeclarationReachability::All,
        );
        assert_eq!(
            declaration.finish(),
            expected_declaration,
            "{path} declaration"
        );
    }
}

#[test]
fn anonymous_default_commonjs_runtime_names_avoid_authored_collisions() {
    let file = program_file(
        "anonymous-default-collision.ts",
        concat!(
            "const default_1 = 0;\n",
            "export default function (): number { return 1; }\n",
        ),
    );
    let mut javascript = Printer::new(
        &file.source,
        &file.bindings,
        &CompilerOptions {
            target: "es2022".to_string(),
            module: "commonjs".to_string(),
            ..CompilerOptions::default()
        },
    );
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            "exports.default = default_2;\n",
            "const default_1 = 0;\n",
            "function default_2() { return 1; }\n",
        ),
    );
}

#[test]
fn default_export_spelling_stays_authored_at_the_esm_boundary() {
    let cases = [
        (
            "escaped-class.ts",
            "export default class C\\u006cassed {}\n",
            "export default class C\\u006cassed {\n}\n",
            "export default class C\\u006cassed {\n}\n",
        ),
        (
            "escaped-function.ts",
            "export default function f\\u0075n(): number { return 1; }\n",
            "export default function f\\u0075n() { return 1; }\n",
            "export default function f\\u0075n(): number;\n",
        ),
    ];

    for (path, source, expected_javascript, expected_declaration) in cases {
        let file = program_file(path, source);
        let options = CompilerOptions {
            declaration: true,
            target: "es2022".to_string(),
            module: "preserve".to_string(),
            ..CompilerOptions::default()
        };
        let mut javascript = Printer::new(&file.source, &file.bindings, &options);
        javascript.emit_javascript(&file.syntax);
        assert_eq!(
            javascript.finish(),
            expected_javascript,
            "{path} JavaScript"
        );
        let mut declaration = Printer::new(&file.source, &file.bindings, &options);
        declaration.emit_declarations(
            &file.syntax,
            &super::reachability::DeclarationReachability::All,
        );
        assert_eq!(
            declaration.finish(),
            expected_declaration,
            "{path} declaration"
        );
    }
}

#[test]
fn ordinary_named_exports_keep_declare_in_declaration_output() {
    let file = program_file(
        "ordinary.ts",
        "export class Ordinary {}\nexport function ordinary(): void {}\n",
    );
    let output = emit_file(
        &file,
        &CompilerOptions {
            declaration: true,
            target: "es2022".to_string(),
            module: "preserve".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(output.len(), 2);
    assert_eq!(
        output[1].text,
        concat!(
            "export declare class Ordinary {\n",
            "}\n",
            "export declare function ordinary(): void;\n",
        )
    );
}

#[test]
fn declaration_accessors_erase_type_parameters_but_preserve_authored_this_parameters() {
    let text = concat!(
        "export declare class Accessors {\n",
        "  get value<T>(): number;\n",
        "  set value<U>(next: number);\n",
        "  method<V>(): V;\n",
        "  get contextual(this: Accessors): number;\n",
        "}\n",
    );
    let source = SourceText::new(
        FileId(0),
        PathBuf::from("accessor-types.ts"),
        Arc::<str>::from(text),
    );
    let parsed = parse_source(&source);
    let bindings = bind_source_with_kind(
        source.id,
        crate::source::SourceKind::TypeScript,
        &parsed.unit,
    );
    let options = CompilerOptions {
        declaration: true,
        target: "es2022".to_string(),
        module: "preserve".to_string(),
        ..CompilerOptions::default()
    };
    let mut declaration = Printer::new(&source, &bindings, &options);
    declaration.emit_declarations(
        &parsed.unit,
        &super::reachability::DeclarationReachability::All,
    );
    assert_eq!(
        declaration.finish(),
        concat!(
            "export declare class Accessors {\n",
            "    get value(): number;\n",
            "    set value(next: number);\n",
            "    method<V>(): V;\n",
            "    get contextual(this: Accessors): number;\n",
            "}\n",
        )
    );
}

#[test]
fn concise_arrows_preserve_object_grouping_when_assertions_are_erased() {
    let file = program_file(
        "arrows.ts",
        concat!(
            "const indexed = (key: string) => ({ one: \"one\" } as unknown)[key];\n",
            "const chained = () => ((({ value: 1 } as unknown))).value;\n",
            "const plain = () => ({ value: 1 });\n",
            "const scalar = () => (value as unknown);\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2015".to_string(),
        module: "preserve".to_string(),
        ..CompilerOptions::default()
    };
    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "const indexed = (key) => ({ one: \"one\" }[key]);\n",
            "const chained = () => ({ value: 1 }.value);\n",
            "const plain = () => ({ value: 1 });\n",
            "const scalar = () => value;\n",
        )
    );
}

#[test]
fn else_if_chains_are_inline_but_explicit_else_blocks_remain_blocks() {
    let file = program_file(
        "branches.ts",
        concat!(
            "if (first) { one(); }\n",
            "else if (second) { two(); }\n",
            "else if (third) { three(); }\n",
            "else { fallback(); }\n",
            "if (outer) { outerCall(); }\n",
            "else { if (nested) { nestedCall(); } }\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2015".to_string(),
        module: "preserve".to_string(),
        ..CompilerOptions::default()
    };
    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "if (first) {\n",
            "    one();\n",
            "}\n",
            "else if (second) {\n",
            "    two();\n",
            "}\n",
            "else if (third) {\n",
            "    three();\n",
            "}\n",
            "else {\n",
            "    fallback();\n",
            "}\n",
            "if (outer) {\n",
            "    outerCall();\n",
            "}\n",
            "else {\n",
            "    if (nested) {\n",
            "        nestedCall();\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn arrow_parameter_parentheses_follow_the_authored_head_and_runtime_requirements() {
    let file = program_file(
        "parameters.ts",
        concat!(
            "const bare = value => value;\n",
            "const nested = call(item => item);\n",
            "const wrapped = (entry => entry);\n",
            "const authored = (value) => value;\n",
            "const typed = (value: number) => value;\n",
            "const defaulted = (value = 1) => value;\n",
            "const multiple = (left, right) => left + right;\n",
        ),
    );
    let options = CompilerOptions {
        target: "es2015".to_string(),
        module: "preserve".to_string(),
        ..CompilerOptions::default()
    };
    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "const bare = value => value;\n",
            "const nested = call(item => item);\n",
            "const wrapped = (entry => entry);\n",
            "const authored = (value) => value;\n",
            "const typed = (value) => value;\n",
            "const defaulted = (value = 1) => value;\n",
            "const multiple = (left, right) => left + right;\n",
        )
    );
}

#[test]
fn explicit_new_type_arguments_erase_with_authored_constructor_structure() {
    let source = concat!(
        "declare const C: any; declare const factory: any;\n",
        "new C;\n",
        "new C();\n",
        "new C<string>();\n",
        "new C<string, number>(1, 2);\n",
        "new C<C<number>>(new C<number>());\n",
        "new namespaceValue.Member<boolean>();\n",
        "new (C)<string>();\n",
        "new (factory())<number>(1);\n",
        "new C /* before-list */ <string>();\n",
        "new C<string> /* after-list */ ();\n",
        "new C<string /* inner */> /* after-inner */ ();\n",
        "new C<number>;\n",
        "new C<number> /* after-no-list */;\n",
        "new C<C<number> /* erased-inner */> /* kept-after */;\n",
        "new C<number> / 2;\n",
        "new C<number> /* before-operator */ / 2;\n",
    );
    let file = program_file("new-types.ts", source);
    let options = CompilerOptions {
        target: "es2022".to_string(),
        ..CompilerOptions::default()
    };
    let mut javascript = Printer::new(&file.source, &file.bindings, &options);
    javascript.emit_javascript(&file.syntax);
    assert_eq!(
        javascript.finish(),
        concat!(
            "\"use strict\";\n",
            "new C;\n",
            "new C();\n",
            "new C();\n",
            "new C(1, 2);\n",
            "new C(new C());\n",
            "new namespaceValue.Member();\n",
            "new (C)();\n",
            "new (factory())(1);\n",
            "new C /* before-list */();\n",
            "new C();\n",
            "new C();\n",
            "new C;\n",
            "new C /* after-no-list */;\n",
            "new C /* kept-after */;\n",
            "new C / 2;\n",
            "new C /* before-operator */ / 2;\n",
        ),
    );
}

#[test]
fn explicit_new_type_arguments_claim_only_javascript_in_checked_and_no_check_emit() {
    let affected = concat!(
        "class Local<Item>{}\n",
        "const Factory=Map;\n",
        "new Local<string>();\n",
        "new Map<string,number>();\n",
        "new Factory<string,number>();\n",
        "export const built:Local<string>=new Local<string>();",
    );
    let stable = "export const stable:number=1;";
    for no_check in [false, true] {
        let options = CompilerOptions {
            declaration: true,
            no_check,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        };
        let file = program_file("affected.ts", affected);
        let capabilities = CapabilityAnalysis::derive(
            std::slice::from_ref(&file),
            &options,
            CapabilityContext::default(),
        );
        let scope = CapabilityScope::File(file.source.id);
        assert!(
            capabilities
                .claim(CapabilityTarget::JavaScript, scope)
                .is_claimed(),
        );
        assert!(
            !capabilities
                .claim(CapabilityTarget::Declaration, scope)
                .is_claimed(),
        );

        let output = Compiler::new().compile(
            vec![
                SourceInput::new("affected.ts", Arc::<str>::from(affected)),
                SourceInput::new("stable.ts", Arc::<str>::from(stable)),
            ],
            &options,
        );
        assert_eq!(output.diagnostics, [], "noCheck={no_check}: {output:#?}");
        let product = |path: &str| {
            output
                .emitted_files
                .iter()
                .find(|file| file.path == Path::new(path))
                .map(|file| file.text.as_str())
        };
        assert_eq!(
            product("affected.js"),
            Some(concat!(
                "class Local {\n",
                "}\n",
                "const Factory = Map;\n",
                "new Local();\n",
                "new Map();\n",
                "new Factory();\n",
                "export const built = new Local();\n",
            )),
        );
        assert_eq!(product("affected.d.ts"), None);
        assert_eq!(
            product("stable.d.ts"),
            Some("export declare const stable: number;\n")
        );
    }
}

#[test]
fn invalid_new_type_argument_lists_remain_parser_owned() {
    let empty = "declare const C:any;new C<>();";
    let parsed = crate::syntax::parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("empty.ts"),
        Arc::<str>::from(empty),
    ));
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [1099],
    );
    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new("empty.ts", Arc::<str>::from(empty))],
            &CompilerOptions {
                no_check,
                ..CompilerOptions::default()
            },
        );
        assert_eq!(output.emitted_files[0].text, "\"use strict\";\nnew C();\n");
    }

    let recovered = "declare const C:any;new C</* leading */ string>();";
    let parsed = crate::syntax::parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("recovered.ts"),
        Arc::<str>::from(recovered),
    ));
    assert!(!parsed.diagnostics.is_empty());
    assert!(!parsed.unit.parser_recovery_facts.is_empty());
    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "recovered.ts",
                Arc::<str>::from(recovered),
            )],
            &CompilerOptions {
                no_check,
                ..CompilerOptions::default()
            },
        );
        assert!(output.emitted_files.is_empty(), "{output:#?}");
    }
}

#[test]
fn default_export_expression_declarations_use_complete_checker_summaries() {
    let cases = [
        (
            "binary",
            "export default 1 + 2;\n",
            concat!(
                "declare const _default: number;\n",
                "export default _default;\n",
            ),
        ),
        (
            "parenthesized",
            "export default (1 + 2);\n",
            concat!(
                "declare const _default: number;\n",
                "export default _default;\n",
            ),
        ),
        (
            "constructed",
            concat!(
                "class Unused {}\n",
                "class A {}\n",
                "export default new A();\n",
            ),
            concat!(
                "declare class A {\n",
                "}\n",
                "declare const _default: A;\n",
                "export default _default;\n",
            ),
        ),
        (
            "literal",
            "export default 3.14159;\n",
            concat!(
                "declare const _default = 3.14159;\n",
                "export default _default;\n",
            ),
        ),
        (
            "string-literal",
            "export default (\"cedar\");\n",
            concat!(
                "declare const _default = \"cedar\";\n",
                "export default _default;\n",
            ),
        ),
        (
            "null-literal",
            "export default null;\n",
            concat!(
                "declare const _default: null;\n",
                "export default _default;\n",
            ),
        ),
        (
            "parenthesized-identifier",
            concat!("const value = 1;\n", "export default (value);\n"),
            concat!(
                "declare const value_1: number;\n",
                "export default value_1;\n",
            ),
        ),
        (
            "named-default-function",
            "export default function foo() { return \"\"; }\n",
            "export default function foo(): string;\n",
        ),
        (
            "anonymous-default-function",
            "export default function () { return 1; }\n",
            "export default function (): number;\n",
        ),
        (
            "anonymous-default-class",
            "export default class {}\n",
            "export default class {\n}\n",
        ),
        (
            "collision",
            concat!(
                "var _default = 1;\n",
                "export { _default as d };\n",
                "export default 1 + 2;\n",
            ),
            concat!(
                "declare var _default: number;\n",
                "export { _default as d };\n",
                "declare const _default_1: number;\n",
                "export default _default_1;\n",
            ),
        ),
        (
            "identifier",
            concat!("const value = 1;\n", "export default value;\n"),
            concat!("declare const value = 1;\n", "export default value;\n"),
        ),
    ];
    for (name, source, expected) in cases {
        for no_check in [false, true] {
            let output = Compiler::new().compile(
                vec![SourceInput::new(
                    format!("{name}.ts"),
                    Arc::<str>::from(source),
                )],
                &CompilerOptions {
                    declaration: true,
                    module: "esnext".to_string(),
                    target: "es2022".to_string(),
                    no_check,
                    ..CompilerOptions::default()
                },
            );
            assert!(
                output.diagnostics.is_empty(),
                "{name} no_check={no_check}: {output:#?}"
            );
            assert_eq!(
                output.semantic_completion,
                crate::program::SemanticCompletion::Complete,
                "{name} no_check={no_check}: {output:#?}",
            );
            assert_eq!(
                output
                    .emitted_files
                    .iter()
                    .find(|file| file.path == Path::new(&format!("{name}.d.ts")))
                    .map(|file| file.text.as_str()),
                Some(expected),
                "{name} no_check={no_check}: {output:#?}",
            );
        }
    }

    let deferred = Compiler::new().compile(
        vec![SourceInput::new(
            "generic.ts",
            Arc::<str>::from(concat!(
                "declare function identity<T>(value: T): T;\n",
                "export default identity(1);\n",
            )),
        )],
        &CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        deferred.semantic_completion,
        crate::program::SemanticCompletion::Deferred,
        "{deferred:#?}",
    );
    assert!(
        deferred
            .emitted_files
            .iter()
            .all(|file| file.path != Path::new("generic.d.ts")),
        "an incomplete default-export operand must not enter a definitive DTS summary: {deferred:#?}",
    );
}

#[test]
fn inferred_object_literal_array_declarations_use_complete_checker_summaries() {
    let source = concat!(
        "export let concrete=[{foo:0,m(){}},{bar:1}];\n",
        "export const wrapped=([({cedar:1}),({birch:\"x\"})]);\n",
        "export let annotated:({oak:number}|{elm:string})[]=[{oak:1},{elm:\"x\"}];\n",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new("arrays.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{output:#?}");
    assert_eq!(
        output.semantic_completion,
        crate::program::SemanticCompletion::Complete
    );
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.path == Path::new("arrays.d.ts"))
        .expect("complete inferred arrays publish their declaration product");
    assert_eq!(
        declaration.text,
        concat!(
            "export declare let concrete: ({\n",
            "    foo: number;\n",
            "    m(): void;\n",
            "    bar?: undefined;\n",
            "} | {\n",
            "    foo?: undefined;\n",
            "    m?: undefined;\n",
            "    bar: number;\n",
            "})[];\n",
            "export declare const wrapped: ({\n",
            "    cedar: number;\n",
            "    birch?: undefined;\n",
            "} | {\n",
            "    cedar?: undefined;\n",
            "    birch: string;\n",
            "})[];\n",
            "export declare let annotated: ({\n",
            "    oak: number;\n",
            "} | {\n",
            "    elm: string;\n",
            "})[];\n",
        )
    );

    let generic = Compiler::new().compile(
        vec![SourceInput::new(
            "generic.ts",
            Arc::<str>::from(concat!(
                "declare function identity<T>(value:T):T;",
                "export let deferred=[{oak:identity(1)},{elm:identity(\"x\")}];",
            )),
        )],
        &CompilerOptions {
            declaration: true,
            module: "esnext".to_string(),
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        generic.semantic_completion,
        crate::program::SemanticCompletion::Deferred
    );
    assert!(
        generic
            .emitted_files
            .iter()
            .all(|file| file.path != Path::new("generic.d.ts")),
        "an incomplete generic operand must not enter a definitive DTS summary: {generic:#?}",
    );
}
