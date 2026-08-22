use std::path::PathBuf;
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
fn template_var_declaration_and_map_products_remain_deferred() {
    for no_check in [false, true] {
        for no_emit in [false, true] {
            for mode in [
                "declaration",
                "declarationMap",
                "declarationDir",
                "sourceMap",
                "inlineSourceMap",
            ] {
                let mut compiler_options = CompilerOptions {
                    no_check,
                    no_emit,
                    ..options("es2015")
                };
                match mode {
                    "declaration" => compiler_options.declaration = true,
                    "declarationMap" => compiler_options.declaration_map = true,
                    "declarationDir" => {
                        compiler_options.declaration_dir = Some(PathBuf::from("types"));
                    }
                    "sourceMap" => compiler_options.source_map = true,
                    "inlineSourceMap" => compiler_options.inline_source_map = true,
                    _ => unreachable!(),
                }
                let output = compile(
                    "product-boundary.ts",
                    "var payload = `plain`;",
                    compiler_options,
                );
                assert_eq!(
                    output.semantic_completion,
                    SemanticCompletion::Deferred,
                    "mode={mode} noCheck={no_check} noEmit={no_emit}: {:?}",
                    output.diagnostics
                );
                assert!(output.emitted_files.is_empty(), "mode={mode}");
            }
        }
    }
}

#[test]
fn nonordinary_binding_names_fail_closed() {
    for no_check in [false, true] {
        for no_emit in [false, true] {
            for source in [
                "var await = `keyword`;",
                "var type = `contextual`;",
                "var using = `contextual`;",
                "var package = `reserved`;",
                "var eval = `strict`;",
                "var arguments = `strict`;",
                r"var \u0078 = `escaped`;",
                "var é = `unicode`;",
            ] {
                let output = compile(
                    "binding-boundary.ts",
                    source,
                    CompilerOptions {
                        no_check,
                        no_emit,
                        ..options("es2015")
                    },
                );
                assert_eq!(
                    output.semantic_completion,
                    SemanticCompletion::Deferred,
                    "{source:?} noCheck={no_check} noEmit={no_emit}: {:?}",
                    output.diagnostics
                );
                assert!(output.emitted_files.is_empty(), "{source:?}");
            }
        }
    }
}

#[test]
fn duplicate_and_selected_library_value_bindings_fail_closed() {
    for no_check in [false, true] {
        for source in [
            "var duplicate = `one`; var duplicate = `two`;",
            "var Array = `selected library collision`;",
        ] {
            let output = compile(
                "collision.ts",
                source,
                CompilerOptions {
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "{source:?} noCheck={no_check}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty());
        }

        let first = SourceInput::new("a.ts", Arc::<str>::from("var shared = `a`;"));
        let second = SourceInput::new("b.ts", Arc::<str>::from("var shared = `b`;"));
        for roots in [
            vec![first.clone(), second.clone()],
            vec![second.clone(), first.clone()],
        ] {
            let output = Compiler::new().compile(
                roots,
                &CompilerOptions {
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty());
        }
    }
}

#[test]
fn declaration_lists_mixed_shapes_and_broader_hosts_fail_closed() {
    for no_check in [false, true] {
        for source in [
            "var first = `one`, second = `two`;",
            "var first, second = `two`;",
            "var first = `one`; `two`;",
            "`one`; var second = `two`;",
            "var first = `one`; const second = `two`;",
            "let first = `one`;",
            "const first = `one`;",
            "var first: string = `one`;",
            "export var first = `one`;",
            "var first = (`one`);",
            "var first = `one`; const sibling = 1;",
            "// preserved comment\nvar first = `one`;",
        ] {
            let output = compile(
                "shape-boundary.ts",
                source,
                CompilerOptions {
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "{source:?} noCheck={no_check}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "{source:?}");
        }
    }

    for path in [
        "variables.js",
        "variables.tsx",
        "variables.mts",
        "variables.d.ts",
    ] {
        let output = compile(
            path,
            "var renamed = `plain`;",
            CompilerOptions {
                allow_js: true,
                no_check: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}: {:?}",
            output.diagnostics
        );
        assert!(output.emitted_files.is_empty(), "{path}");
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
