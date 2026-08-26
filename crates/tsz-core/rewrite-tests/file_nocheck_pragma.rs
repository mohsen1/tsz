use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::service::LanguageService;
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

type DiagnosticFingerprint = (String, u32, u32, u32, DiagnosticCategory, String);

fn options() -> CompilerOptions {
    CompilerOptions {
        no_emit: true,
        target: "es2015".to_string(),
        ..CompilerOptions::default()
    }
}

fn input(path: &str, source: &str) -> SourceInput {
    SourceInput::new(path, Arc::<str>::from(source.to_string()))
}

fn diagnostic_fingerprint(output: &tsz::CompileOutput) -> Vec<DiagnosticFingerprint> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            assert!(diagnostic.related_information.is_empty());
            (
                diagnostic.file.clone(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.clone(),
            )
        })
        .collect()
}

fn semantic_fingerprint(
    result: &tsz::service::SemanticDiagnosticResult,
) -> Vec<DiagnosticFingerprint> {
    result
        .diagnostics
        .iter()
        .map(|diagnostic| {
            assert!(diagnostic.related_information.is_empty());
            (
                diagnostic.file.clone(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.clone(),
            )
        })
        .collect()
}

fn mismatch_fingerprint(path: &str, source: &str) -> DiagnosticFingerprint {
    (
        path.to_string(),
        2322,
        source.find("value").expect("declaration name") as u32,
        "value".len() as u32,
        DiagnosticCategory::Error,
        "Type 'number' is not assignable to type 'string'.".to_string(),
    )
}

#[test]
fn direct_trailing_text_and_colon_forms_match_ts7() {
    for (name, directive) in [
        ("direct", "// @ts-nocheck"),
        ("trailing", "// @ts-nocheck additional comments"),
        ("colon", "// @ts-nocheck: additional comments"),
        ("triple-case", "///\t@TS-NOCHECK trailing"),
    ] {
        let source = format!("{directive}\nconst hidden: string = 1;\n");
        let output = Compiler::new().compile(vec![input("case.ts", &source)], &options());
        assert!(
            output.diagnostics.is_empty(),
            "{name}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{name}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{name}");
        assert_eq!(output.stats.types, 0, "{name}");
    }
}

#[test]
fn last_leading_check_control_directive_wins() {
    let checked = concat!(
        "// @ts-nocheck\n",
        "// @ts-check\n",
        "const value: string = 1;\n",
    );
    let checked_output = Compiler::new().compile(vec![input("checked.ts", checked)], &options());
    assert_eq!(
        diagnostic_fingerprint(&checked_output),
        vec![mismatch_fingerprint("checked.ts", checked)]
    );
    assert_eq!(
        checked_output.semantic_completion,
        SemanticCompletion::Complete
    );

    let unchecked = concat!(
        "// @ts-check\n",
        "// @ts-nocheck\n",
        "const value: string = 1;\n",
    );
    let unchecked_output =
        Compiler::new().compile(vec![input("unchecked.ts", unchecked)], &options());
    assert!(unchecked_output.diagnostics.is_empty());
    assert_eq!(
        unchecked_output.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(unchecked_output.stats.types, 0);
}

#[test]
fn non_leading_and_near_miss_comments_do_not_disable_checking() {
    for (name, source) in [
        ("non-leading", "const value: string = 1;\n// @ts-nocheck\n"),
        (
            "wrong-name",
            "// @ts-nochecking\nconst value: string = 1;\n",
        ),
        (
            "block-comment",
            "/* @ts-nocheck */\nconst value: string = 1;\n",
        ),
        (
            "four-slashes",
            "//// @ts-nocheck\nconst value: string = 1;\n",
        ),
    ] {
        let output = Compiler::new().compile(vec![input("case.ts", source)], &options());
        assert_eq!(
            diagnostic_fingerprint(&output),
            vec![mismatch_fingerprint("case.ts", source)],
            "{name}"
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{name}"
        );
        assert!(output.stats.types > 0, "{name}");
    }
}

#[test]
fn file_scope_and_cross_file_demands_are_root_order_invariant() {
    let unchecked = concat!(
        "// @ts-nocheck: source-owned\n",
        "const shared: string = 1;\n",
        "const hidden: number = \"wrong\";\n",
    );
    let checked = "const copy: number = shared;\n";
    let roots = vec![
        input("unchecked.ts", unchecked),
        input("checked.ts", checked),
    ];
    let mut reversed = roots.clone();
    reversed.reverse();

    let forward = Compiler::new().compile(roots, &options());
    let reverse = Compiler::new().compile(reversed, &options());
    let expected = vec![(
        "checked.ts".to_string(),
        2322,
        checked.find("copy").expect("declaration name") as u32,
        "copy".len() as u32,
        DiagnosticCategory::Error,
        "Type 'string' is not assignable to type 'number'.".to_string(),
    )];
    assert_eq!(diagnostic_fingerprint(&forward), expected);
    assert_eq!(diagnostic_fingerprint(&reverse), expected);
    assert_eq!(forward.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(reverse.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(forward.stats.types, reverse.stats.types);
    assert!(forward.stats.types > 0);

    let mut service = LanguageService::new(options());
    service.open("unchecked.ts", Arc::<str>::from(unchecked));
    service.open("checked.ts", Arc::<str>::from(checked));
    let hidden = service.semantic_diagnostics("unchecked.ts");
    assert!(hidden.diagnostics.is_empty());
    assert_eq!(hidden.semantic_completion, SemanticCompletion::Complete);
    let visible = service.semantic_diagnostics("checked.ts");
    assert_eq!(semantic_fingerprint(&visible), expected);
    assert_eq!(visible.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        semantic_fingerprint(&service.semantic_diagnostics("checked.ts")),
        expected
    );
}

#[test]
fn unchecked_unannotated_producer_keeps_its_inferred_type_for_checked_consumers() {
    let unchecked = concat!(
        "// @ts-nocheck\n",
        "const shared = 1;\n",
        "const hidden: string = 1;\n",
    );
    let checked = "const copy: string = shared;\n";
    let roots = vec![
        input("unchecked.ts", unchecked),
        input("checked.ts", checked),
    ];
    let mut reversed = roots.clone();
    reversed.reverse();
    let compiler = Compiler::new();
    let forward = compiler.compile(roots.clone(), &options());
    let repeated = compiler.compile(roots, &options());
    let reverse = compiler.compile(reversed, &options());
    let expected = vec![(
        "checked.ts".to_string(),
        2322,
        checked.find("copy").expect("consumer declaration") as u32,
        "copy".len() as u32,
        DiagnosticCategory::Error,
        "Type 'number' is not assignable to type 'string'.".to_string(),
    )];
    for output in [&forward, &repeated, &reverse] {
        assert_eq!(diagnostic_fingerprint(output), expected);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsSkipped
        );
    }
    assert_eq!(forward.stats.types, repeated.stats.types);
    assert_eq!(forward.stats.types, reverse.stats.types);
}

#[test]
fn checked_consumer_completes_through_an_owned_unchecked_template_initializer() {
    let unchecked = concat!(
        "// @ts-nocheck\n",
        "const shared = `plain`;\n",
        "const hidden: string = 1;\n",
    );
    let checked = "const copy: string = shared;\n";
    let roots = vec![
        input("unchecked.ts", unchecked),
        input("checked.ts", checked),
    ];
    let mut reversed = roots.clone();
    reversed.reverse();
    let compiler = Compiler::new();
    let forward = compiler.compile(roots.clone(), &options());
    let repeated = compiler.compile(roots, &options());
    let reverse = compiler.compile(reversed, &options());
    for output in [&forward, &repeated, &reverse] {
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(output.exit_status, CompileExitStatus::Success);
    }
    assert_eq!(forward.stats.types, repeated.stats.types);
    assert_eq!(forward.stats.types, reverse.stats.types);
}

#[test]
fn syntax_diagnostics_remain_owned_in_an_unchecked_file() {
    let source = "// @ts-nocheck\nconst broken: = 1;\n";
    let output = Compiler::new().compile(vec![input("syntax.ts", source)], &options());
    assert_eq!(
        diagnostic_fingerprint(&output),
        vec![(
            "syntax.ts".to_string(),
            1110,
            source.find('=').expect("recovered type") as u32,
            1,
            DiagnosticCategory::Error,
            "Type expected.".to_string(),
        )]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
    assert_eq!(output.stats.types, 0);
}

#[test]
fn javascript_and_declaration_products_continue_without_semantic_checking() {
    let source = "// @ts-nocheck\nexport const value: number = \"wrong\";\n";
    let output = Compiler::new().compile(
        vec![input("product.ts", source)],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    assert_eq!(output.stats.types, 0);
    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("JavaScript product");
    assert_eq!(javascript.path.to_string_lossy(), "product.js");
    assert!(javascript.text.contains("export const value = \"wrong\";"));
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration product");
    assert_eq!(declaration.path.to_string_lossy(), "product.d.ts");
    assert_eq!(declaration.text, "export declare const value: number;\n");
}

#[test]
fn ts7_trailing_text_corpus_witnesses_complete() {
    for directive in [
        "// @ts-nocheck additional comments",
        "// @ts-nocheck: additional comments",
    ] {
        let source = format!(
            "{directive}\n\nexport const a = 1 + {{}};\n\nexport interface Aleph {{\n  q: number;\n}}\n\nexport class Bet implements Aleph {{\n  q: string = 'lol';\n}}\n"
        );
        let output = Compiler::new().compile(
            vec![input("file.ts", &source)],
            &CompilerOptions {
                declaration: true,
                // Keep pragma recognition independent of the separately
                // nonclaimed pre-ES2022 class-field transform.
                target: "es2022".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert!(
            output.diagnostics.is_empty(),
            "{directive}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{directive}"
        );
        assert_eq!(
            output.exit_status,
            CompileExitStatus::Success,
            "{directive}"
        );
        assert!(output.emitted_files.iter().any(|file| !file.declaration));
        assert!(output.emitted_files.iter().any(|file| file.declaration));
    }
}

#[test]
fn javascript_check_mode_and_source_directives_share_one_file_owner() {
    let missing = "MissingName;\n";
    for (name, check_js, directive, expected) in [
        ("default", None, "", false),
        ("option-off", Some(false), "", false),
        ("option-on", Some(true), "", true),
        ("directive-on", Some(false), "// @ts-check\n", true),
        ("directive-off", Some(true), "// @ts-nocheck\n", false),
    ] {
        let source = format!("{directive}{missing}");
        let output = Compiler::new().compile(
            vec![input("case.js", &source)],
            &CompilerOptions {
                allow_js: true,
                check_js,
                no_emit: true,
                ..CompilerOptions::default()
            },
        );
        let diagnostics = diagnostic_fingerprint(&output);
        if expected {
            assert_eq!(
                diagnostics,
                vec![(
                    "case.js".to_string(),
                    2304,
                    directive.len() as u32,
                    "MissingName".len() as u32,
                    DiagnosticCategory::Error,
                    "Cannot find name 'MissingName'.".to_string(),
                )],
                "{name}",
            );
            assert!(output.stats.types > 0, "{name}");
        } else {
            assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
            assert_eq!(output.stats.types, 0, "{name}");
        }
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{name}"
        );
    }
}

#[test]
fn unchecked_javascript_still_supplies_types_to_checked_typescript() {
    let javascript = "const shared = 1; MissingHidden;\n";
    let typescript = "const copy: string = shared;\n";
    let mut roots = vec![
        input("producer.js", javascript),
        input("consumer.ts", typescript),
    ];
    let compiler_options = CompilerOptions {
        allow_js: true,
        check_js: Some(false),
        no_emit: true,
        ..CompilerOptions::default()
    };
    for name in ["forward", "reverse"] {
        let output = Compiler::new().compile(roots.clone(), &compiler_options);
        assert_eq!(
            diagnostic_fingerprint(&output),
            vec![(
                "consumer.ts".to_string(),
                2322,
                typescript.find("copy").unwrap() as u32,
                "copy".len() as u32,
                DiagnosticCategory::Error,
                "Type 'number' is not assignable to type 'string'.".to_string(),
            )],
            "{name}",
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{name}"
        );
        roots.reverse();
    }
}
