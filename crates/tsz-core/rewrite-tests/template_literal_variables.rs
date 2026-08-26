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

fn codes(output: &CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
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
fn control_and_backslash_escape_var_rows_preserve_exact_javascript() {
    let sources = [
        r"var x = `\0\x00\u0000 0 00 0000`;",
        r"var x = `\x19\u0019 19`;",
        r"var x = `\x1F\u001f 1F 1f`;",
        r"var x = `\x20\u0020 20`;",
        concat!(
            r"var a = `hello\world`;",
            "\n",
            r"var b = `hello\\world`;",
            "\n",
            r"var c = `hello\\\world`;",
            "\n",
            r"var d = `hello\\\\world`;",
        ),
    ];

    for target in ["es2015", "es6"] {
        for no_check in [false, true] {
            for source in sources {
                let output = compile(
                    "renamed-control.ts",
                    source,
                    CompilerOptions {
                        no_check,
                        ..options(target)
                    },
                );
                assert_eq!(
                    output.semantic_completion,
                    SemanticCompletion::Complete,
                    "target={target} noCheck={no_check} {source:?}: {:?}",
                    output.diagnostics
                );
                assert_eq!(output.exit_status, CompileExitStatus::Success);
                assert!(output.diagnostics.is_empty(), "{source:?}");
                assert_eq!(javascript(&output), format!("\"use strict\";\n{source}\n"));
            }
        }
    }
}

#[test]
fn ordinary_renamed_var_bindings_and_no_emit_are_owned() {
    let source = concat!(
        "var renamed9 = `first`;\n",
        "var _payload = `second`;\n",
        "var $payload = `third`;",
    );
    for no_check in [false, true] {
        for module in ["commonjs", "esnext", "preserve"] {
            let output = compile(
                "ordinary-bindings.ts",
                source,
                CompilerOptions {
                    module: module.to_string(),
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "module={module} noCheck={no_check}: {:?}",
                output.diagnostics
            );
            assert!(output.diagnostics.is_empty());
            assert_eq!(
                javascript(&output),
                concat!(
                    "\"use strict\";\n",
                    "var renamed9 = `first`;\n",
                    "var _payload = `second`;\n",
                    "var $payload = `third`;\n",
                )
            );
        }

        let no_emit = compile(
            "ordinary-bindings.ts",
            source,
            CompilerOptions {
                no_check,
                no_emit: true,
                ..options("es2015")
            },
        );
        assert_eq!(no_emit.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(no_emit.exit_status, CompileExitStatus::Success);
        assert!(no_emit.diagnostics.is_empty());
        assert!(no_emit.emitted_files.is_empty());
    }
}

#[test]
fn distinct_variable_roots_are_repeated_and_order_stable() {
    let first = SourceInput::new("b.ts", Arc::<str>::from(r"var beta = `\x42`;"));
    let second = SourceInput::new("a.ts", Arc::<str>::from(r"var alpha = `\x41`;"));

    let fingerprint = |output: &CompileOutput| {
        (
            output.semantic_completion,
            output.exit_status,
            codes(output),
            output
                .emitted_files
                .iter()
                .map(|file| {
                    (
                        file.path.to_string_lossy().into_owned(),
                        file.text.clone(),
                        file.declaration,
                    )
                })
                .collect::<Vec<_>>(),
            output.stats.identifiers,
            output.stats.symbols,
            output.stats.types,
        )
    };

    for no_check in [false, true] {
        let compiler = Compiler::new();
        let compiler_options = CompilerOptions {
            no_check,
            ..options("es2015")
        };
        let expected =
            fingerprint(&compiler.compile(vec![first.clone(), second.clone()], &compiler_options));
        assert_eq!(expected.0, SemanticCompletion::Complete);
        assert_eq!(expected.1, CompileExitStatus::Success);

        for iteration in 0..10 {
            let roots = if iteration % 2 == 0 {
                vec![first.clone(), second.clone()]
            } else {
                vec![second.clone(), first.clone()]
            };
            let actual = compiler.compile(roots, &compiler_options);
            assert_eq!(
                fingerprint(&actual),
                expected,
                "noCheck={no_check} iteration={iteration}"
            );
        }
    }
}

#[test]
fn no_lib_diagnostics_remain_owned_for_template_variables() {
    for no_check in [false, true] {
        let output = compile(
            "no-lib.ts",
            "var renamed = `plain`;",
            CompilerOptions {
                no_check,
                no_lib: true,
                ..options("es2015")
            },
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(codes(&output), vec![2318; 10]);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        );
        assert_eq!(
            javascript(&output),
            "\"use strict\";\nvar renamed = `plain`;\n"
        );
    }
}
