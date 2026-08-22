use std::path::PathBuf;
use std::sync::Arc;

use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    ExpressionKind, Literal, NoSubstitutionTemplateLiteral, StatementKind, parse_source,
};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn compile(path: &str, source: &str, options: CompilerOptions) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(path, Arc::<str>::from(source))],
        &options,
    )
}

fn options(target: &str) -> CompilerOptions {
    CompilerOptions {
        target: target.to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    }
}

fn codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn javascript(output: &tsz::CompileOutput) -> &str {
    output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .unwrap_or_else(|| {
            panic!(
                "expected one JavaScript product: {:?} {:?}",
                output.semantic_completion, output.diagnostics
            )
        })
        .text
        .as_str()
}

fn parse_template(raw: &str) -> NoSubstitutionTemplateLiteral {
    let source = SourceText::new(
        FileId(0),
        PathBuf::from("syntax.ts"),
        Arc::<str>::from(format!("const payload = {raw};")),
    );
    let parsed = parse_source(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("expected one statement");
    };
    let StatementKind::Variable(declaration) = &statement.kind else {
        panic!("expected a variable declaration");
    };
    let Some(initializer) = &declaration.initializer else {
        panic!("expected an initializer");
    };
    let ExpressionKind::Literal(Literal::NoSubstitutionTemplate(literal)) = &initializer.kind
    else {
        panic!("expected a no-substitution template literal");
    };
    literal.clone()
}

#[test]
fn syntax_retains_exact_raw_token_and_cooked_value() {
    let authored = r"`\x41\u0042\u{43}\0\t\world\${literal}`";
    let literal = parse_template(authored);
    assert_eq!(literal.raw, authored);
    assert_eq!(literal.cooked, "ABC\0\tworld${literal}");

    for (raw, cooked) in [
        ("`\r\n\\\r\n`", "\n"),
        ("`\n\\\n`", "\n"),
        ("`\r\\\r`", "\n"),
        ("`a\\\r\nb`", "ab"),
    ] {
        let literal = parse_template(raw);
        assert_eq!(literal.raw, raw);
        assert_eq!(literal.cooked, cooked);
    }
}

#[test]
fn sixteen_logical_shapes_and_es2015_es6_twins_preserve_exact_tokens() {
    let shapes = [
        r"`\0\x00\u0000 0 00 0000`",
        r"`\x19\u0019 19`",
        r"`\x1F\u001f 1F 1f`",
        r"`\x20\u0020 20`",
        "`\r\n\\\r\n`",
        "`\n\\\n`",
        "`\r\\\r`",
        "``",
        r"`\\`",
        r"`\``",
        r"`\\\\`",
        r"`\\\\\\`",
        r"`0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 2028 2029 0085 t v f b r n`",
        r"`\t\n\v\f\r`",
        r"`\u0009\u000B\u000C\u0020\u00A0\uFEFF`",
        r"`hello\world hello\\world hello\\\world hello\\\\world`",
    ];
    assert_eq!(shapes.len(), 16);

    for target in ["es2015", "es6"] {
        for raw in shapes {
            let source = format!("{raw};");
            let output = compile("shape.ts", &source, options(target));
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "{target} {raw:?}: {:?}",
                output.diagnostics
            );
            assert_eq!(output.exit_status, CompileExitStatus::Success, "{raw:?}");
            assert!(output.diagnostics.is_empty(), "{raw:?}");
            assert_eq!(javascript(&output), format!("\"use strict\";\n{raw};\n"));
        }
    }
}

#[test]
fn safe_file_boundary_allows_only_direct_expression_statements() {
    for source in [
        "  \n`first`;\r\n`second`;\n",
        "`// inside the template is syntax, not trivia`;",
    ] {
        let output = compile("safe.ts", source, options("es2015"));
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source:?}: {:?}",
            output.diagnostics
        );
        assert!(output.diagnostics.is_empty(), "{source:?}");
        assert!(!output.emitted_files.is_empty(), "{source:?}");
    }

    for source in [
        "; `plain`;",
        "`expression`; var value = `variable`;",
        "const value = `plain`;",
        "let value = `plain`;",
        "var value: string = `plain`;",
        "export var value = `plain`;",
        "\"use strict\"; `plain`;",
        "/* leading comment */ var value = `plain`;",
        "#!/usr/bin/env node\n`plain`;",
    ] {
        let output = compile(
            "outside-safe-file.ts",
            source,
            CompilerOptions {
                no_check: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source:?}: {:?}",
            output.diagnostics
        );
        assert!(output.emitted_files.is_empty(), "{source:?}");
    }

    let declaration_request = compile(
        "direct-expression.ts",
        "`plain`;",
        CompilerOptions {
            declaration: true,
            no_check: true,
            ..options("es2015")
        },
    );
    assert_eq!(
        declaration_request.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        javascript(&declaration_request),
        "\"use strict\";\n`plain`;\n"
    );
    let declarations = declaration_request
        .emitted_files
        .iter()
        .filter(|file| file.declaration)
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].text, "");
}

#[test]
fn variable_declarations_other_source_kinds_and_broader_hosts_defer() {
    let js = compile(
        "renamed.js",
        r"`hello\world`;",
        CompilerOptions {
            allow_js: true,
            no_check: true,
            out_dir: Some(PathBuf::from("out")),
            ..options("es2015")
        },
    );
    assert_eq!(js.semantic_completion, SemanticCompletion::Deferred);
    assert!(js.emitted_files.is_empty());

    for path in [
        "direct.tsx",
        "direct.mts",
        "direct.cts",
        "direct.TS",
        "ambient.d.ts",
        "renamed-declarations.d.ts",
    ] {
        let output = compile(
            path,
            r"`hello\world`;",
            CompilerOptions {
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

    for no_check in [false, true] {
        for source in [
            "var duplicate = `one`; var duplicate = `two`;",
            "var await = `keyword`;",
            "var Array = `library collision`;",
        ] {
            let output = compile(
                "variable.ts",
                source,
                CompilerOptions {
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "{source:?}");
        }
    }

    let ts_source = concat!(
        r#"function rename(value: string): string { return value; }"#,
        r#"const renamed = rename((`\x41`));"#,
    );
    let ts = compile("nested.ts", ts_source, options("es2015"));
    assert_eq!(ts.semantic_completion, SemanticCompletion::Deferred);
    assert!(ts.emitted_files.is_empty());

    let unchecked = compile(
        "unchecked.ts",
        r"const exact: number = `\x41`;",
        CompilerOptions {
            no_check: true,
            ..options("es2015")
        },
    );
    assert!(unchecked.diagnostics.is_empty());
    assert_eq!(unchecked.semantic_completion, SemanticCompletion::Deferred);
    assert!(unchecked.emitted_files.is_empty());
}

#[test]
fn tagged_adjacent_interpolated_and_type_forms_remain_deferred() {
    for source in [
        "tag`plain`;",
        "tag\n`plain`;",
        "tag()`plain`;",
        "tag<string>`plain`;",
        r"tag`bad \xG0`;",
        r"tag!`bad \xG0`;",
        r"async`bad \xG0`;",
        r"declare`bad \xG0`;",
        r"abstract`bad \xG0`;",
        "`first``second`;",
        "tag`head ${value} tail`;",
        "`head ${value} tail`;",
        "type Template = `plain`;",
    ] {
        let output = compile(
            "unsupported.ts",
            source,
            CompilerOptions {
                declaration: true,
                no_check: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source:?}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.emitted_files.is_empty(), "{source:?}");
    }

    for source in [
        "tag`plain`;",
        "tag\n`plain`;",
        r"tag`bad \xG0`;",
        r"tag!`bad \xG0`;",
        r"async`bad \xG0`;",
        r"declare`bad \xG0`;",
        r"abstract`bad \xG0`;",
        "`first``second`;",
    ] {
        let output = compile(
            "clean-unsupported.ts",
            source,
            CompilerOptions {
                no_emit: true,
                no_check: true,
                ..options("es2015")
            },
        );
        assert!(
            output.diagnostics.is_empty(),
            "{source:?}: {:?}",
            output.diagnostics
        );
    }

    let malformed_javascript = compile(
        "unsupported.js",
        "tag!`plain`;",
        CompilerOptions {
            allow_js: true,
            no_check: true,
            ..options("es2015")
        },
    );
    assert_eq!(
        malformed_javascript.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(malformed_javascript.emitted_files.is_empty());

    let javascript_non_null = compile(
        "ordinary.js",
        "value!();",
        CompilerOptions {
            allow_js: true,
            no_emit: true,
            no_check: true,
            ..options("es2015")
        },
    );
    assert!(!javascript_non_null.diagnostics.is_empty());
}

#[test]
fn ordinary_non_null_assertions_are_not_silently_erased() {
    for source in [
        "const value: number = null!;",
        concat!(
            "declare const maybe: string | undefined; ",
            "const value: string = maybe!;",
        ),
        "const value = (null)!();",
        "const value = null!!;",
        "const value = factory<string>!;",
    ] {
        let output = compile(
            "non-null.ts",
            source,
            CompilerOptions {
                no_check: true,
                ..options("es2015")
            },
        );
        assert!(
            !output.diagnostics.is_empty()
                || output.semantic_completion != SemanticCompletion::Complete,
            "ordinary non-null syntax was silently accepted: {source:?}"
        );
    }

    let tagged = compile(
        "tagged-non-null.ts",
        r"tag!`bad \xG0`;",
        CompilerOptions {
            no_emit: true,
            no_check: true,
            ..options("es2015")
        },
    );
    assert_eq!(tagged.semantic_completion, SemanticCompletion::Deferred);
    assert!(tagged.diagnostics.is_empty(), "{:?}", tagged.diagnostics);
}

#[test]
fn await_template_contexts_defer_until_await_grammar_is_owned() {
    // TS7 reports TS2304 in the script and ordinary-function contexts, while
    // module top-level and async-function await are clean. Until the parser
    // owns that context split, every direct await/template pair fails closed.
    let contexts = [
        ("script.ts", "", ";"),
        ("module.ts", "export const marker = 0; ", ";"),
        ("ordinary.ts", "function task() { ", "; }"),
        ("async.ts", "async function task() { ", "; }"),
    ];
    for (path, prefix, suffix) in contexts {
        for raw in [
            "`plain`",
            r"`bad \xG0`",
            "(`plain`)",
            "((`plain`))",
            "[`plain`]",
        ] {
            let source = format!("{prefix}await {raw}{suffix}");
            let output = compile(
                path,
                &source,
                CompilerOptions {
                    declaration: true,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "{source:?}: {:?}",
                output.diagnostics
            );
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
            assert!(output.emitted_files.is_empty());
            assert!(
                codes(&output)
                    .iter()
                    .all(|code| !matches!(code, 1125 | 1198 | 1199 | 1487 | 1488)),
                "{source:?}: {:?}",
                output.diagnostics
            );
        }
    }
}

#[test]
fn unrelated_await_hosts_stay_outside_the_safe_file_boundary() {
    let sources = [
        "function task(p: Promise<void>) { await p; } const shown = `plain`;",
        "async function task(p: Promise<void>) { await p; } const shown = `plain`;",
        "export const marker = 0; await promise; const shown = `plain`;",
        "export default await promise; const shown = `plain`;",
    ];
    for no_check in [false, true] {
        for source in sources {
            let output = compile(
                "await-source.ts",
                source,
                CompilerOptions {
                    declaration: true,
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "{source:?}");
        }
    }
}

#[test]
fn template_dependent_unowned_expression_hosts_fail_closed() {
    let sources = [
        "const result = `x` - 1;",
        "const result = 1 - `x`;",
        "const result = `x` < 1;",
        "const result = `x` === 1;",
        "const result = `x` in {};",
        "const result = `x` instanceof Object;",
        "delete `x`;",
        "!`x`;",
        "`x` && true;",
        "`x`, `y`;",
        "`x` as number;",
        "new `x`;",
        "class Crate { constructor(value: number) {} } new Crate(`x`);",
        "if (`x`) {}",
        "if (``) {}",
        "switch (value) { case `x`: break; }",
        "switch (`x`) { case 1: break; }",
        "`x` = value;",
        "(`x`) = value;",
        "[`x`] = value;",
        "({ value: `x` }) = source;",
        "class Box { field: number = `x`; }",
        "class Box { method(): number { return `x`; } }",
        "class Box { constructor() { const value: number = `x`; } }",
    ];
    for no_check in [false, true] {
        for source in sources {
            let output = compile(
                "unowned.ts",
                source,
                CompilerOptions {
                    declaration: true,
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                output.diagnostics
            );
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
            assert!(output.emitted_files.is_empty(), "{source:?}");
        }
    }
}

#[test]
fn syntax_recovery_anywhere_in_a_template_source_fails_closed() {
    let sources = [
        "function `x`() {}",
        "import `x`;",
        "while (`x`) {}",
        "try { `x`; } catch {}",
        "const value = true ? `x` : `y`;",
        "let value = 0; value += `x`;",
        "switch (value) { `x`; }",
    ];
    for no_check in [false, true] {
        for source in sources {
            let output = compile(
                "recovered.ts",
                source,
                CompilerOptions {
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "{source:?}");
        }
    }
}

#[test]
fn jump_and_cross_line_satisfies_template_detachments_fail_closed() {
    for no_check in [false, true] {
        for source in [
            "break `x`;",
            "continue `x`;",
            "break label `x`;",
            "continue label `x`;",
            "const result = `x`\nsatisfies \"y\";",
        ] {
            let output = compile(
                "detached.ts",
                source,
                CompilerOptions {
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "{source:?}");
        }
    }
}

#[test]
fn assignment_call_member_and_return_hosts_stay_outside_the_safe_file_boundary() {
    let source = concat!(
        "let assigned = \"\";",
        "`direct`;",
        "assigned = `assignment`;",
        "function accept(value: string): string { return value; }",
        "accept(`call`);",
        "const length = `member`.length;",
        "function reveal(): string { return `return`; }",
    );
    let output = compile(
        "owned.ts",
        source,
        CompilerOptions {
            no_check: true,
            ..options("es2015")
        },
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(output.emitted_files.is_empty());
}

#[test]
fn unowned_template_tails_and_non_template_siblings_defer() {
    for source in [
        "const foo = 1; `x` foo;",
        "const picked: string = `x`[foo];",
        "const foo = 0; const picked = `x`\n[foo];",
        "`x`++;",
        "`x`?.length;",
        "`x`\nvalue;",
    ] {
        let output = compile(
            "tail.ts",
            source,
            CompilerOptions {
                no_check: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source:?}: {:?}",
            output.diagnostics
        );
        assert!(output.emitted_files.is_empty(), "{source:?}");
    }

    for source in ["`x`", "`x`;"] {
        let output = compile(
            "asi.ts",
            source,
            CompilerOptions {
                no_check: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source:?}: {:?}",
            output.diagnostics
        );
        assert!(javascript(&output).contains("`x`"));
    }
}

#[test]
fn ascii_ordinary_invalid_escape_diagnostics_match_ts7_under_no_check() {
    let hexadecimal = "Hexadecimal digit expected.";
    let extended = "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.";
    let cases = [
        (r"`\x`", 1125, 3, 1, hexadecimal),
        (r"`\x0`", 1125, 4, 1, hexadecimal),
        (r"`\xG0`", 1125, 3, 1, hexadecimal),
        (r"`\x0G`", 1125, 4, 1, hexadecimal),
        (r"`\u`", 1125, 3, 1, hexadecimal),
        (r"`\u0`", 1125, 4, 1, hexadecimal),
        (r"`\u00`", 1125, 5, 1, hexadecimal),
        (r"`\u000`", 1125, 6, 1, hexadecimal),
        (r"`\u00G0`", 1125, 5, 1, hexadecimal),
        (r"`\u{}`", 1125, 4, 1, hexadecimal),
        (r"`\u{G}`", 1125, 4, 1, hexadecimal),
        (r"`\u{110000}`", 1198, 4, 6, extended),
        (r"`\u{FFFFFF}`", 1198, 4, 6, extended),
        (
            r"`\u{10FFFF`",
            1199,
            10,
            1,
            "Unterminated Unicode escape sequence.",
        ),
        (r"`\8`", 1488, 1, 2, "Escape sequence '\\8' is not allowed."),
        (r"`\9`", 1488, 1, 2, "Escape sequence '\\9' is not allowed."),
        (
            r"`\1`",
            1487,
            1,
            2,
            "Octal escape sequences are not allowed. Use the syntax '\\x01'.",
        ),
        (
            r"`\01`",
            1487,
            1,
            3,
            "Octal escape sequences are not allowed. Use the syntax '\\x01'.",
        ),
    ];
    for (source, code, start, length, message) in cases {
        let output = compile(
            "invalid.ts",
            source,
            CompilerOptions {
                no_emit: true,
                no_check: true,
                ..options("es2015")
            },
        );
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!(
                "expected one diagnostic for {source:?}: {:?}",
                output.diagnostics
            );
        };
        assert_eq!(
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ),
            (code, start, length, message),
            "{source:?}"
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn unicode_prefixed_invalid_escapes_defer_without_span_ownership() {
    for source in [r"`é\xG0`;", r"`😀\u{}`;"] {
        let output = compile(
            "unicode-invalid.ts",
            source,
            CompilerOptions {
                declaration: true,
                no_check: true,
                ..options("es2015")
            },
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.emitted_files.is_empty());
    }
}

#[test]
fn valid_edge_unicode_and_unrepresentable_surrogates_do_not_substitute() {
    for source in [r"`\x00`;", r"`\u0000`;", r"`\u{10FFFF}`;"] {
        let output = compile("valid.ts", source, options("es2015"));
        assert!(output.diagnostics.is_empty(), "{source:?}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(javascript(&output), format!("\"use strict\";\n{source}\n"));
    }
    for source in [r"`\uD800`;", r"`\u{D800}`;"] {
        let output = compile(
            "surrogate.ts",
            source,
            CompilerOptions {
                no_check: true,
                ..options("es2015")
            },
        );
        assert!(output.diagnostics.is_empty(), "{source:?}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(output.emitted_files.is_empty());
    }
}

#[test]
fn return_hosts_stay_outside_the_safe_file_boundary_even_across_asi() {
    let output = compile(
        "asi.ts",
        "function take() { return\n`after`; }",
        CompilerOptions {
            no_check: true,
            ..options("es2015")
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert!(output.emitted_files.is_empty());

    let typed = compile(
        "returns.ts",
        concat!(
            "export function separated(): void { return\n`after`; }",
            "export function same(): \"same\" { return `same`; }",
        ),
        CompilerOptions {
            declaration: true,
            ..options("es2015")
        },
    );
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    assert_eq!(typed.semantic_completion, SemanticCompletion::Deferred);
    assert!(typed.emitted_files.is_empty());
}

#[test]
fn unmodeled_template_programs_skip_speculative_semantic_diagnostics() {
    let cases: [(&str, &str, &[u32]); 2] = [
        (
            "stringLiteralTypesWithTemplateStrings01.ts",
            concat!(
                "let ABC: \"ABC\" = `ABC`;\n",
                "let DE_NEWLINE_F: \"DE\\nF\" = `DE\nF`;\n",
                "let G_QUOTE_HI: 'G\"HI';\n",
                "let JK_BACKTICK_L: \"JK`L\" = `JK\\`L`;",
            ),
            &[],
        ),
        (
            "stringLiteralTypesWithTemplateStrings02.ts",
            concat!(
                "let abc: \"AB\\r\\nC\" = `AB\nC`;\n",
                "let de_NEWLINE_f: \"DE\\nF\" = `DE${\"\\n\"}F`;",
            ),
            &[1109, 1109],
        ),
    ];

    for no_check in [false, true] {
        for (path, source, expected_codes) in cases {
            let output = compile(
                path,
                source,
                CompilerOptions {
                    declaration: true,
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {path}: {:?}",
                output.diagnostics
            );
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
            assert_eq!(output.stats.types, 0, "noCheck={no_check} {path}");
            assert_eq!(codes(&output), expected_codes, "noCheck={no_check} {path}");
            assert!(output.emitted_files.is_empty(), "noCheck={no_check} {path}");
        }
    }

    let interpolated_only = compile(
        "interpolated-only.ts",
        "let value: string = `head${missing}tail`;",
        CompilerOptions {
            declaration: true,
            ..options("es2015")
        },
    );
    assert_eq!(
        interpolated_only.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        interpolated_only.exit_status,
        CompileExitStatus::SemanticIncomplete
    );
    assert_eq!(interpolated_only.stats.types, 0);
    assert!(
        interpolated_only
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2304),
        "{:?}",
        interpolated_only.diagnostics
    );
    assert!(interpolated_only.emitted_files.is_empty());
}

#[test]
fn es5_is_rejected_and_unmodeled_declaration_literal_emit_is_deferred() {
    for (target, code) in [("es5", 5108), ("unsupported", 6046)] {
        let removed_target = compile(
            "removed.ts",
            "`plain`;",
            CompilerOptions {
                no_check: true,
                ..options(target)
            },
        );
        assert_eq!(codes(&removed_target), vec![code], "{target}");
        assert_eq!(
            removed_target.semantic_completion,
            SemanticCompletion::Deferred,
            "{target}"
        );
        assert!(removed_target.emitted_files.is_empty(), "{target}");
    }

    for source in [
        "export const value = `plain`;",
        "export function reveal() { return `plain`; }",
        "export function reveal() { const value = `plain`; return value; }",
        concat!(
            "export function reveal() { ",
            "function inner() { return `plain`; } return inner; }",
        ),
    ] {
        let declaration = compile(
            "declaration.ts",
            source,
            CompilerOptions {
                declaration: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            declaration.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert_eq!(
            declaration.exit_status,
            CompileExitStatus::SemanticIncomplete
        );
        assert!(declaration.emitted_files.is_empty());
    }

    for source in [
        concat!(
            "export function reveal() { ",
            "class Local { value = `plain`; } return Local; }",
        ),
        concat!(
            "export function reveal() { ",
            "class Local { method() { return `plain`; } } return Local; }",
        ),
    ] {
        let declaration = compile(
            "class-declaration.ts",
            source,
            CompilerOptions {
                declaration: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            declaration.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert!(declaration.emitted_files.is_empty());
    }

    for no_check in [false, true] {
        let default_export = compile(
            "default.ts",
            r"export default `\x41`;",
            CompilerOptions {
                declaration: true,
                no_check,
                ..options("es2015")
            },
        );
        assert_eq!(
            default_export.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert!(default_export.emitted_files.is_empty());
    }

    for no_check in [false, true] {
        for source in [
            "export function outer() { export default `plain`; }",
            "export function outer() { export const inner = `plain`; }",
            "export default (() => { export default `plain`; });",
            concat!(
                "function outer(arg = () => { export default `plain`; }) ",
                "{ return arg; }",
            ),
            concat!(
                "class Outer { method(arg = () => { export const value = 1; }) {} } ",
                "const marker = `plain`;",
            ),
            "function outer() { `plain`; export { name }; }",
            "const marker = `plain`; function outer() { export function inner() {} }",
            "const marker = `plain`; function outer() { export class Inner {} }",
            "const marker = `plain`; function outer() { export interface Inner {} }",
            "const marker = `plain`; function outer() { export type Inner = string; }",
            "function outer() { `plain`; import { value } from \"pkg\"; }",
            "function outer() { `plain`; import \"pkg\"; }",
            "function outer() { `plain`; import type { Value } from \"pkg\"; }",
            "function outer() { `plain`; import value = require(\"pkg\"); }",
            "function outer() { declare const hidden: number; } const shown = `plain`;",
            "const renamed = `plain`; function container() { declare function hidden(): void; }",
            concat!(
                "function outer() { function inner() { declare type Alias = string; } } ",
                "const shown = `plain`;",
            ),
            "declare tag`plain`;",
        ] {
            let nested_export = compile(
                "nested-export.ts",
                source,
                CompilerOptions {
                    declaration: true,
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                nested_export.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                nested_export.diagnostics
            );
            assert!(nested_export.emitted_files.is_empty(), "{source:?}");
        }

        let root_ambient = compile(
            "root-ambient.ts",
            "declare const ambient: number; const shown = `plain`;",
            CompilerOptions {
                no_check,
                ..options("es2015")
            },
        );
        assert_eq!(
            root_ambient.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert!(root_ambient.emitted_files.is_empty());

        for source in [
            "declare function ambient(): void; const shown = `plain`;",
            "declare class Ambient {} const shown = `plain`;",
            "declare interface Ambient {} const shown = `plain`;",
            "declare type Alias = string; const shown = `plain`;",
        ] {
            let root_host = compile(
                "root-owned-ambient.ts",
                source,
                CompilerOptions {
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                root_host.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                root_host.diagnostics
            );
            assert!(root_host.emitted_files.is_empty());
        }
    }

    let annotated = compile(
        "annotated.ts",
        concat!(
            "export function stable(): () => string { ",
            "function inner(): string { return `plain`; } return inner; }",
        ),
        CompilerOptions {
            declaration: true,
            ..options("es2015")
        },
    );
    assert_eq!(annotated.semantic_completion, SemanticCompletion::Deferred);
    assert!(
        annotated.diagnostics.is_empty(),
        "{:?}",
        annotated.diagnostics
    );
    assert!(annotated.emitted_files.is_empty());
}

#[test]
fn unowned_statement_modifier_hosts_in_template_sources_fail_closed() {
    let deferred_sources = [
        "export default async function task() { return `plain`; }",
        "export default abstract class Box {} const marker = `plain`;",
        "export default function task() { return `plain`; }",
        "export default class Box {} const marker = `plain`;",
        "export default interface Shape {} const marker = `plain`;",
        "export default type Alias = string; const marker = `plain`;",
        "export default let value = 1; const marker = `plain`;",
        "export default const value = 1; const marker = `plain`;",
        "export default var value = 1; const marker = `plain`;",
        "export default declare type Alias = string; const marker = `plain`;",
        "export default async const value = 1; const marker = `plain`;",
        "export default export class Box {} const marker = `plain`;",
        "export default export default type Alias = string; const marker = `plain`;",
        "export default namespace Space {} const marker = `plain`;",
        "export default module Space {} const marker = `plain`;",
        "export default global {} const marker = `plain`;",
        "export default enum Choice {} const marker = `plain`;",
        "export default using resource = value; const marker = `plain`;",
        "export default import value = require(\"pkg\"); const marker = `plain`;",
        "async function task() { return `plain`; }",
        "abstract class Box {} const marker = `plain`;",
        "async const value = 1; const marker = `plain`;",
        "abstract const value = 1; const marker = `plain`;",
        "async type Alias = string; const marker = `plain`;",
        "abstract interface Shape {} const marker = `plain`;",
        "abstract value; const marker = `plain`;",
        "declare export function task(): void; const marker = `plain`;",
        "export export function task(): void; const marker = `plain`;",
        "declare declare function task(): void; const marker = `plain`;",
        "async async function task() {} const marker = `plain`;",
        "abstract abstract class Box {} const marker = `plain`;",
        "export export const value = 1; const marker = `plain`;",
        "declare export value; const marker = `plain`;",
    ];
    for no_check in [false, true] {
        for module in ["esnext", "commonjs"] {
            for source in deferred_sources {
                let output = compile(
                    "modifier-host.ts",
                    source,
                    CompilerOptions {
                        declaration: true,
                        no_check,
                        module: module.to_string(),
                        ..options("es2015")
                    },
                );
                assert_eq!(
                    output.semantic_completion,
                    SemanticCompletion::Deferred,
                    "noCheck={no_check} module={module} {source:?}: {:?}",
                    output.diagnostics
                );
                assert!(output.emitted_files.is_empty(), "{source:?}");
            }
        }

        for source in [
            "export const shown = `plain`;",
            "declare function ambient(): void; const shown = `plain`;",
            concat!(
                "export declare function ambient(): void; ",
                "export const shown = `plain`;",
            ),
            "export type Alias = string; export const shown = `plain`;",
            "export interface Shape {} export const shown = `plain`;",
        ] {
            let output = compile(
                "modifier-host-outside-safe-file.ts",
                source,
                CompilerOptions {
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty());
        }
    }

    let async_source = SourceText::new(
        FileId(0),
        PathBuf::from("default-async.ts"),
        Arc::<str>::from("export default async function task() { return `plain`; }"),
    );
    let async_parse = parse_source(&async_source);
    assert!(async_parse.diagnostics.is_empty());
    assert!(matches!(
        &async_parse.unit.statements[0].kind,
        StatementKind::Function(declaration)
            if declaration.default_export && declaration.is_async
    ));

    let abstract_source = SourceText::new(
        FileId(0),
        PathBuf::from("default-abstract.ts"),
        Arc::<str>::from("export default abstract class Box {} const marker = `plain`;"),
    );
    let abstract_parse = parse_source(&abstract_source);
    assert!(abstract_parse.diagnostics.is_empty());
    assert!(matches!(
        &abstract_parse.unit.statements[0].kind,
        StatementKind::Class(declaration)
            if declaration.default_export && declaration.abstract_class
    ));
}

#[test]
fn resource_and_contextual_using_hosts_stay_outside_the_safe_file_boundary() {
    for no_check in [false, true] {
        for source in [
            "using resource = acquire(); const shown = `plain`;",
            "await using resource = acquire(); const shown = `plain`;",
            "export default using resource = acquire(); const shown = `plain`;",
            "export default await using resource = acquire(); const shown = `plain`;",
        ] {
            let output = compile(
                "resource.ts",
                source,
                CompilerOptions {
                    declaration: true,
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "{source:?}");
        }
    }

    for source in [
        "using; const shown = `plain`;",
        "using(value); const shown = `plain`;",
    ] {
        let output = compile(
            "contextual-using.ts",
            source,
            CompilerOptions {
                no_check: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source:?}: {:?}",
            output.diagnostics
        );
        assert!(output.emitted_files.is_empty());
    }
}

#[test]
fn silent_statement_splits_and_unrelated_asi_siblings_defer() {
    for no_check in [false, true] {
        for source in [
            "readonly class Box {} const shown = `plain`;",
            "value other; const shown = `plain`;",
            "export default readonly class Box {} const shown = `plain`;",
        ] {
            let output = compile(
                "same-line-split.ts",
                source,
                CompilerOptions {
                    declaration: true,
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {source:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "{source:?}");
        }

        let newline_asi = compile(
            "newline-asi.ts",
            "value\nother; const shown = `plain`;",
            CompilerOptions {
                no_check,
                ..options("es2015")
            },
        );
        assert_eq!(
            newline_asi.semantic_completion,
            SemanticCompletion::Deferred,
            "noCheck={no_check}: {:?}",
            newline_asi.diagnostics
        );
        assert!(newline_asi.emitted_files.is_empty());
    }
}

#[test]
fn export_default_contextual_expressions_remain_assignments() {
    for source in [
        "export default import(\"pkg\");",
        "export default await promise;",
        "export default type;",
        "export default module;",
        "export default namespace;",
        "export default global;",
        "export default using;",
        "export default using(value);",
    ] {
        let source = SourceText::new(
            FileId(0),
            PathBuf::from("contextual-default.ts"),
            Arc::<str>::from(source),
        );
        let parsed = parse_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(matches!(
            &parsed.unit.statements[0].kind,
            StatementKind::Export(declaration) if declaration.assignment.is_some()
        ));
    }

    for source in [
        "export default import(\"pkg\"); const shown = `plain`;",
        "export default type; const shown = `plain`;",
        "export default module; const shown = `plain`;",
        "export default namespace; const shown = `plain`;",
        "export default global; const shown = `plain`;",
        "export default using; const shown = `plain`;",
        "export default using(value); const shown = `plain`;",
    ] {
        let source = SourceText::new(
            FileId(0),
            PathBuf::from("contextual-default-with-template.ts"),
            Arc::<str>::from(source),
        );
        let parsed = parse_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(matches!(
            &parsed.unit.statements[0].kind,
            StatementKind::Export(declaration) if declaration.assignment.is_some()
        ));
    }
}

#[test]
fn unsupported_export_default_declaration_hosts_defer_without_template_syntax() {
    for no_check in [false, true] {
        for no_emit in [false, true] {
            for source in [
                "export default const value = 1;",
                "export default type Alias = string;",
                "export default export class C {}",
            ] {
                let output = compile(
                    "unsupported-default-host.ts",
                    source,
                    CompilerOptions {
                        declaration: true,
                        no_check,
                        no_emit,
                        ..options("es2015")
                    },
                );
                assert_eq!(
                    output.semantic_completion,
                    SemanticCompletion::Deferred,
                    "noCheck={no_check} noEmit={no_emit} {source:?}: {:?}",
                    output.diagnostics
                );
                assert!(output.emitted_files.is_empty(), "{source:?}");
            }
        }
    }

    for source in [
        "export default function named() {}",
        "export default class Named {}",
    ] {
        let source = SourceText::new(
            FileId(0),
            PathBuf::from("owned-default-host.ts"),
            Arc::<str>::from(source),
        );
        let parsed = parse_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(
            matches!(
                &parsed.unit.statements[0].kind,
                StatementKind::Function(declaration) if declaration.default_export
            ) || matches!(
                &parsed.unit.statements[0].kind,
                StatementKind::Class(declaration) if declaration.default_export
            )
        );

        for no_check in [false, true] {
            let output = compile(
                "owned-default-host.ts",
                source.text.as_ref(),
                CompilerOptions {
                    no_check,
                    no_emit: true,
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "noCheck={no_check}: {:?}",
                output.diagnostics
            );
        }
    }

    for source in ["export default import(\"pkg\");", "export default type;"] {
        let output = compile(
            "contextual-default.ts",
            source,
            CompilerOptions {
                no_check: true,
                ..options("es2015")
            },
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source:?}: {:?}",
            output.diagnostics
        );
        assert!(javascript(&output).contains(source));
    }
}

#[test]
fn repeated_compiles_and_both_root_orders_have_one_product_fingerprint() {
    let first = SourceInput::new("b.ts", Arc::<str>::from(r"`\x42`;"));
    let second = SourceInput::new("a.ts", Arc::<str>::from(r"`\x41`;"));
    let options = CompilerOptions {
        no_check: true,
        target: "es2015".to_string(),
        module: "esnext".to_string(),
        ..CompilerOptions::default()
    };
    let fingerprint = |output: &tsz::CompileOutput| {
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
        )
    };
    let expected =
        fingerprint(&Compiler::new().compile(vec![first.clone(), second.clone()], &options));
    assert_eq!(expected.0, SemanticCompletion::Complete);
    assert_eq!(expected.1, CompileExitStatus::Success);

    for iteration in 0..10 {
        let roots = if iteration % 2 == 0 {
            vec![first.clone(), second.clone()]
        } else {
            vec![second.clone(), first.clone()]
        };
        let actual = Compiler::new().compile(roots, &options);
        assert_eq!(fingerprint(&actual), expected, "iteration {iteration}");
    }
}

#[test]
fn template_safe_file_boundary_is_program_wide_and_map_modes_defer() {
    let template = SourceInput::new("template.ts", Arc::<str>::from("`plain`;"));
    let sibling = SourceInput::new("sibling.ts", Arc::<str>::from("missing;"));
    for no_check in [false, true] {
        for roots in [
            vec![template.clone(), sibling.clone()],
            vec![sibling.clone(), template.clone()],
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
            let expected_codes: &[u32] = if no_check { &[] } else { &[2304] };
            assert_eq!(codes(&output), expected_codes, "noCheck={no_check}");
            assert!(output.emitted_files.is_empty());
        }

        for map_mode in [
            "sourceMap",
            "inlineSourceMap",
            "declarationMap",
            "declarationDir",
        ] {
            let mut map_options = CompilerOptions {
                no_check,
                ..options("es2015")
            };
            match map_mode {
                "sourceMap" => map_options.source_map = true,
                "inlineSourceMap" => map_options.inline_source_map = true,
                "declarationMap" => map_options.declaration_map = true,
                "declarationDir" => map_options.declaration_dir = Some(PathBuf::from("types")),
                _ => unreachable!(),
            }
            let output = Compiler::new().compile(vec![template.clone()], &map_options);
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} {map_mode}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "{map_mode}");
        }
    }
}

#[test]
fn template_program_requires_exact_context_free_compiler_options() {
    let template = SourceInput::new("options.ts", Arc::<str>::from("`plain`;"));
    for no_check in [false, true] {
        for (target, module) in [
            ("es2015", "commonjs"),
            ("ES2025", "ESNEXT"),
            ("esnext", "preserve"),
        ] {
            let output = Compiler::new().compile(
                vec![template.clone()],
                &CompilerOptions {
                    no_check,
                    module: module.to_string(),
                    ..options(target)
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "noCheck={no_check} target={target:?} module={module:?}: {:?}",
                output.diagnostics
            );
            assert_eq!(
                output
                    .emitted_files
                    .iter()
                    .filter(|file| !file.declaration)
                    .count(),
                1
            );
        }

        for module in [
            "node16", "node18", "node20", "nodenext", "cjs", "amd", " esnext", "esnext ",
        ] {
            let output = Compiler::new().compile(
                vec![template.clone()],
                &CompilerOptions {
                    no_check,
                    module: module.to_string(),
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} module={module:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "module={module:?}");
        }

        for target in ["es7", "latest", " es2015", "es2015 "] {
            let output = Compiler::new().compile(
                vec![template.clone()],
                &CompilerOptions {
                    no_check,
                    ..options(target)
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} target={target:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "target={target:?}");
        }

        for libraries in [vec![], vec!["es2015"], vec![" es2015"], vec!["unknown"]] {
            let output = Compiler::new().compile(
                vec![template.clone()],
                &CompilerOptions {
                    no_check,
                    lib: Some(
                        libraries
                            .iter()
                            .map(|library| (*library).to_string())
                            .collect(),
                    ),
                    ..options("es2015")
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} lib={libraries:?}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "lib={libraries:?}");
        }

        let no_emit = Compiler::new().compile(
            vec![template.clone()],
            &CompilerOptions {
                no_check,
                no_emit: true,
                module: "node16".to_string(),
                ..options("es2015")
            },
        );
        assert_eq!(no_emit.semantic_completion, SemanticCompletion::Deferred);
        assert!(no_emit.emitted_files.is_empty());
    }
}
