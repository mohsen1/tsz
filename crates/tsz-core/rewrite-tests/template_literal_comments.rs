use std::sync::Arc;

use tsz::{
    CompileExitStatus, CompileOutput, Compiler, CompilerOptions, SemanticCompletion, SourceInput,
};

fn options(target: &str) -> CompilerOptions {
    CompilerOptions {
        target: target.to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    }
}

fn compile(path: &str, source: &str, options: CompilerOptions) -> CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(path, Arc::<str>::from(source))],
        &options,
    )
}

fn javascript(output: &CompileOutput) -> &str {
    output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .unwrap_or_else(|| {
            panic!(
                "expected JavaScript product: {:?} {:?}",
                output.semantic_completion, output.diagnostics
            )
        })
        .text
        .as_str()
}

#[test]
fn eight_comment_bearing_template_rows_preserve_position_and_line_endings() {
    let shapes = [
        ("// newlines are <CR><LF>", "`\r\n\\\r\n`", "\r\n"),
        ("// newlines are <LF>", "`\n\\\n`", "\n"),
        ("// newlines are <CR>", "`\r\\\r`", "\r"),
        (
            "// <TAB>, <VT>, <FF>, <SP>, <NBSP>, <BOM>",
            r"`\u0009\u000B\u000C\u0020\u00A0\uFEFF`;",
            "\r\n",
        ),
    ];

    for target in ["es2015", "es6"] {
        for module in ["commonjs", "esnext", "preserve"] {
            for no_check in [false, true] {
                for (comment, template, line_break) in shapes {
                    let leading_breaks = if target == "es2015" { 2 } else { 1 };
                    let source = format!(
                        "{}{comment}{line_break}{template}",
                        line_break.repeat(leading_breaks)
                    );
                    let output = compile(
                        "comment-row.ts",
                        &source,
                        CompilerOptions {
                            module: module.to_string(),
                            no_check,
                            ..options(target)
                        },
                    );
                    assert_eq!(
                        output.semantic_completion,
                        SemanticCompletion::Complete,
                        "target={target} module={module} noCheck={no_check} {source:?}: {:?}",
                        output.diagnostics
                    );
                    assert_eq!(output.exit_status, CompileExitStatus::Success);
                    assert!(output.diagnostics.is_empty());
                    let terminator = if template.ends_with(';') { "" } else { ";" };
                    assert_eq!(
                        javascript(&output),
                        format!("\"use strict\";\n{comment}\n{template}{terminator}\n")
                    );
                }
            }
        }
    }
}

#[test]
fn one_leading_comment_is_positioned_before_its_template_statement() {
    let source = "\r\n\r\n// before the template\r`first`;";
    for no_check in [false, true] {
        let output = compile(
            "positions.ts",
            source,
            CompilerOptions {
                no_check,
                ..options("es2015")
            },
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "noCheck={no_check}: {:?}",
            output.diagnostics
        );
        assert_eq!(
            javascript(&output),
            concat!(
                "\"use strict\";\n",
                "// before the template\n",
                "`first`;\n",
            )
        );
    }
}

#[test]
fn unicode_line_comment_terminators_fail_closed_for_every_program_product() {
    for no_check in [false, true] {
        for no_emit in [false, true] {
            for declaration in [false, true] {
                for separator in ['\u{2028}', '\u{2029}'] {
                    for source in [
                        format!("// ordinary{separator}\"use strict\";"),
                        format!("// ordinary{separator}declare const value: number;"),
                        format!("// ordinary{separator}enum Kind {{ First }}"),
                        format!("// ordinary{separator}`plain`;"),
                        format!("// ordinary{separator}declare const value: number;\n`plain`;"),
                        format!("#!/usr/bin/env node{separator}declare const value: number;"),
                        format!("#!/usr/bin/env node{separator}`plain`;"),
                    ] {
                        let output = compile(
                            "unicode-comment-tail.ts",
                            &source,
                            CompilerOptions {
                                strict: true,
                                no_check,
                                no_emit,
                                declaration,
                                remove_comments: true,
                                ..options("es2015")
                            },
                        );
                        assert_eq!(
                            output.semantic_completion,
                            SemanticCompletion::Deferred,
                            "{source:?} noCheck={no_check} noEmit={no_emit} declaration={declaration}: {:?}",
                            output.diagnostics
                        );
                        assert!(output.emitted_files.is_empty(), "{source:?}");
                    }
                }
            }
        }
    }

    let unsafe_root = SourceInput::new(
        "unsafe.ts",
        Arc::<str>::from("// ordinary\u{2028}declare const hidden: number;"),
    );
    let safe_root = SourceInput::new("safe.ts", Arc::<str>::from("// safe\nconst visible = 1;"));
    for roots in [
        vec![unsafe_root.clone(), safe_root.clone()],
        vec![safe_root, unsafe_root],
    ] {
        let output = Compiler::new().compile(roots, &options("es2015"));
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(output.emitted_files.is_empty());
    }
}

#[test]
fn remove_comments_declaration_and_no_emit_products_stay_exact() {
    let source = "// ordinary @ mention\r\n`plain`;";
    for no_check in [false, true] {
        let removed = compile(
            "removed.ts",
            source,
            CompilerOptions {
                no_check,
                remove_comments: true,
                ..options("es2015")
            },
        );
        assert_eq!(removed.semantic_completion, SemanticCompletion::Complete);
        assert!(removed.diagnostics.is_empty());
        assert_eq!(javascript(&removed), "\"use strict\";\n`plain`;\n");

        let declaration = compile(
            "declaration.ts",
            source,
            CompilerOptions {
                no_check,
                declaration: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            declaration.semantic_completion,
            SemanticCompletion::Complete
        );
        assert_eq!(
            javascript(&declaration),
            "\"use strict\";\n// ordinary @ mention\n`plain`;\n"
        );
        let declarations = declaration
            .emitted_files
            .iter()
            .filter(|file| file.declaration)
            .collect::<Vec<_>>();
        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0].text, "");

        let no_emit = compile(
            "no-emit.ts",
            source,
            CompilerOptions {
                no_check,
                no_emit: true,
                ..options("es2015")
            },
        );
        assert_eq!(no_emit.semantic_completion, SemanticCompletion::Complete);
        assert!(no_emit.diagnostics.is_empty());
        assert!(no_emit.emitted_files.is_empty());
    }
}
