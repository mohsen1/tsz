use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

const MESSAGE: &str = "'const' declarations must be initialized.";

fn compile(path: &str, source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(path, Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    )
}

#[test]
fn nonambient_uninitialized_consts_report_exact_order_spans_and_message() {
    let source = concat!(
        "const first: number, ready = 1, renamed: string;\n",
        "function wrapper<Element>() { const nested: Element; }\n",
    );
    let output = compile("uninitialized.ts", source);
    let diagnostics = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 1155)
        .collect::<Vec<_>>();

    assert_eq!(output.diagnostics.len(), 3, "{:#?}", output.diagnostics);
    assert_eq!(diagnostics.len(), 3, "{:#?}", output.diagnostics);
    for (diagnostic, name) in diagnostics.iter().zip(["first", "renamed", "nested"]) {
        assert_eq!(diagnostic.category, DiagnosticCategory::Error);
        assert_eq!(diagnostic.message_text, MESSAGE);
        assert_eq!(
            (diagnostic.start, diagnostic.length),
            (
                source.find(name).expect("diagnostic name") as u32,
                name.len() as u32,
            ),
        );
    }
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn initialized_const_let_var_and_declared_const_are_exempt() {
    let source = concat!(
        "const ready = 1;\n",
        "let mutable: number;\n",
        "var legacy: number;\n",
        "declare const ambient: number;\n",
    );
    let output = compile("controls.ts", source);

    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
}

#[test]
fn declaration_source_is_ambient_without_an_authored_declare_modifier() {
    for path in ["ambient.d.ts", "component.d.html.ts", "data.d.json.ts"] {
        let output = compile(path, "export const declared_by_file: number;\n");

        assert!(
            output.diagnostics.is_empty(),
            "{path}: {:#?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn arbitrary_extension_declaration_sources_do_not_emit_javascript() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "component.d.html.ts",
            Arc::<str>::from("export const declared_by_file: number;\n"),
        )],
        &CompilerOptions::default(),
    );

    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert!(
        output.emitted_files.is_empty(),
        "{:#?}",
        output.emitted_files
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn declaration_source_case_and_basename_boundaries_remain_runtime_sources() {
    for (path, host_path) in [
        ("component.D.html.ts", None),
        (r"folder.d.parts\ordinary.ts", None),
        ("http://server.d.ts", Some("runtime.ts")),
        ("file://server.d.html.ts", Some("runtime.ts")),
        ("//server.d.ts", Some("runtime.ts")),
    ] {
        let text = Arc::<str>::from("export const runtime_value: number;\n");
        let missing = Compiler::new().compile(
            vec![host_path.map_or_else(
                || SourceInput::new(path, Arc::clone(&text)),
                |host| SourceInput::with_host_path(path, host, Arc::clone(&text)),
            )],
            &CompilerOptions {
                no_emit: true,
                strict: true,
                target: "es2015".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(
            missing.diagnostics.len(),
            1,
            "{path}: {:#?}",
            missing.diagnostics
        );
        assert_eq!(missing.diagnostics[0].code, 1155);

        let text = Arc::<str>::from("export const runtime_value: number = 1;\n");
        let emitted = Compiler::new().compile(
            vec![host_path.map_or_else(
                || SourceInput::new(path, Arc::clone(&text)),
                |host| SourceInput::with_host_path(path, host, Arc::clone(&text)),
            )],
            &CompilerOptions::default(),
        );
        assert!(
            emitted.diagnostics.is_empty(),
            "{path}: {:#?}",
            emitted.diagnostics
        );
        assert!(
            emitted
                .emitted_files
                .iter()
                .any(|file| !file.declaration && file.text.contains("runtime_value")),
            "{path}: {:#?}",
            emitted.emitted_files,
        );
    }
}

#[test]
fn recovered_for_of_binding_remains_nonclaimed() {
    let source = concat!(
        "declare const items: Array<{ value: number }>;\n",
        "for (const { value: recovered } of items) { recovered; }\n",
    );
    let output = compile("for-of-recovery.ts", source);

    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 1155),
        "{:#?}",
        output.diagnostics,
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}
