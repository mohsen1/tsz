use std::path::Path;
use std::sync::Arc;

use tsz::source::{FileId, SourceText};
use tsz::syntax::{StatementKind, parse_source};
use tsz::{Compiler, CompilerOptions, SourceInput};

fn compile(source: &str, declaration: bool) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            declaration,
            target: "es2025".to_string(),
            module: "preserve".to_string(),
            ..CompilerOptions::default()
        },
    )
}

fn emit(source: &str) -> String {
    let output = compile(source, false);
    assert!(
        output.diagnostics.is_empty(),
        "identifier-shaped keywords should remain syntax nodes: {:?}",
        output.diagnostics
    );
    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("JavaScript output");
    assert_eq!(javascript.path, Path::new("case.js"));
    javascript.text.clone()
}

#[test]
fn authored_identifier_escapes_survive_javascript_emit_at_every_modeled_node() {
    let output = emit(concat!(
        "class C\\u0032 {\n",
        "  m\\u0033(p\\u0034) { const l\\u0035 = p\\u0034; return this.m\\u0033(l\\u0035); }\n",
        "}\n",
        "const o\\u0036 = { def\\u0061ult: C\\u0032 };\n",
        "o\\u0036.def\\u0061ult(C\\u0032);\n",
    ));

    assert_eq!(
        output,
        concat!(
            "\"use strict\";\n",
            "class C\\u0032 {\n",
            "    m\\u0033(p\\u0034) {\n",
            "        const l\\u0035 = p\\u0034;\n",
            "        return this.m\\u0033(l\\u0035);\n",
            "    }\n",
            "}\n",
            "const o\\u0036 = { def\\u0061ult: C\\u0032 };\n",
            "o\\u0036.def\\u0061ult(C\\u0032);\n",
        )
    );
}

#[test]
fn authored_identifier_escapes_survive_declaration_emit_without_changing_identity() {
    let output = compile(
        concat!(
            "export declare class C\\u0032<T\\u0033> {\n",
            "  m\\u0034(p\\u0035: T\\u0033): T\\u0033;\n",
            "}\n",
            "export type A\\u0036<T\\u0037> = { prop\\u0038: T\\u0037 };\n",
        ),
        true,
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration output");
    assert_eq!(
        declaration.text,
        concat!(
            "export declare class C\\u0032<T\\u0033> {\n",
            "    m\\u0034(p\\u0035: T\\u0033): T\\u0033;\n",
            "}\n",
            "export type A\\u0036<T\\u0037> = {\n",
            "    prop\\u0038: T\\u0037;\n",
            "};\n",
        )
    );
}

#[test]
fn explicit_redundant_module_aliases_survive_cooked_name_equality() {
    let source = concat!(
        "import { \\u0061 as a, b as \\u0062 } from \"./m\";\n",
        "export { \\u0061 as a, b as \\u0062 };\n",
    );
    let output = Compiler::new().compile(
        vec![
            SourceInput::new(
                "m.ts",
                Arc::<str>::from("export const a = 1;\nexport const b = 2;\n"),
            ),
            SourceInput::new("case.ts", Arc::<str>::from(source)),
        ],
        &CompilerOptions {
            no_check: true,
            declaration: true,
            target: "es2025".to_string(),
            module: "preserve".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);

    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration && file.path == Path::new("case.js"))
        .expect("JavaScript output");
    assert_eq!(javascript.text, source);
}

#[test]
fn commonjs_escaped_identifier_transform_spelling_is_exact() {
    let source = concat!(
        "import { \\u0061 as local\\u0062, c as \\u0064 } from \"./m\";\n",
        "export { local\\u0062 as \\u0065, \\u0064 as f };\n",
        "export const a\\u0062 = 1;\n",
        "export function f\\u0063() { return 1; }\n",
        "export class C\\u0064 {}\n",
    );
    let output = Compiler::new().compile(
        vec![
            SourceInput::new(
                "m.ts",
                Arc::<str>::from("export const a = 1;\nexport const c = 2;\n"),
            ),
            SourceInput::new("case.ts", Arc::<str>::from(source)),
        ],
        &CompilerOptions {
            no_check: true,
            target: "es2025".to_string(),
            module: "commonjs".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration && file.path == Path::new("case.js"))
        .expect("JavaScript output");
    assert_eq!(
        javascript.text,
        concat!(
            "\"use strict\";\n",
            "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
            "exports.Cd = void 0;\n",
            "exports.fc = fc;\n",
            "const localb = require(\"./m\").a;\n",
            "const d = require(\"./m\").c;\n",
            "Object.defineProperty(exports, \"\\\\u0065\", { enumerable: true, get: function () { return localb; } });\n",
            "Object.defineProperty(exports, \"f\", { enumerable: true, get: function () { return d; } });\n",
            "const a\\u0062 = 1;\n",
            "exports.a\\u0062 = a\\u0062;\n",
            "function fc() {\n",
            "    return 1;\n",
            "}\n",
            "class Cd {\n",
            "}\n",
            "exports.Cd = Cd;\n",
        )
    );
}

#[test]
fn future_reserved_bindings_keep_their_source_spelling_through_emit() {
    let output = emit(concat!(
        "function yield(package) {\n",
        "  function renamed(static) { return static; }\n",
        "  return package;\n",
        "}\n",
        "yield(package);\n",
    ));

    assert_eq!(
        output,
        concat!(
            "\"use strict\";\n",
            "function yield(package) {\n",
            "    function renamed(static) {\n",
            "        return static;\n",
            "    }\n",
            "    return package;\n",
            "}\n",
            "yield(package);\n",
        )
    );
    assert!(!output.contains("<missing>"));
}

#[test]
fn renamed_and_nested_identifier_shaped_keywords_follow_the_same_rule() {
    for (function_name, parameter) in [
        ("implements", "private"),
        ("interface", "protected"),
        ("let", "public"),
    ] {
        let source = format!(
            "function outer(): void {{ function {function_name}({parameter}) {{ return {parameter}; }} }}\n"
        );
        let output = emit(&source);
        assert!(
            output.contains(&format!("function {function_name}({parameter})")),
            "binding spelling was not preserved: {output}"
        );
        assert!(output.contains(&format!("return {parameter};")));
        assert!(!output.contains("<missing>"));
    }
}

#[test]
fn hard_reserved_words_do_not_become_binding_identifiers() {
    for reserved in ["return", "function", "class"] {
        let text = format!("function {reserved}() {{}}\n");
        let source = SourceText::new(FileId(0), "case.ts".into(), Arc::<str>::from(text));
        let parsed = parse_source(&source);
        assert_eq!(
            parsed.diagnostics.first().map(|diagnostic| diagnostic.code),
            Some(1003),
            "hard reserved word {reserved} was accepted as a binding"
        );
        let StatementKind::Function(declaration) = &parsed.unit.statements[0].kind else {
            panic!("malformed function did not retain its declaration shape");
        };
        assert_eq!(declaration.name, "<missing>");
    }
}
