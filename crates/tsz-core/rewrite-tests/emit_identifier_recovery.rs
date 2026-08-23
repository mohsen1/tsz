use std::path::Path;
use std::sync::Arc;

use tsz::source::{FileId, SourceText};
use tsz::syntax::{StatementKind, parse_source};
use tsz::{Compiler, CompilerOptions, SourceInput};

fn emit(source: &str) -> String {
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            target: "es2025".to_string(),
            module: "preserve".to_string(),
            ..CompilerOptions::default()
        },
    );
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
