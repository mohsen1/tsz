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
    let Some(initializer) = &declaration.declarators[0].initializer else {
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
fn ordinary_non_null_assertions_are_modeled_while_tagged_adjacency_stays_deferred() {
    for source in [
        "const value: number = null!;",
        concat!(
            "declare const maybe: string | undefined; ",
            "const value: string = maybe!;",
        ),
        "const value = (null)!();",
        "const value = null!!;",
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
            output.diagnostics.is_empty(),
            "{source:?}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source:?}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{source:?}");
        assert_eq!(output.emitted_files.len(), 1, "{source:?}");
    }

    let invalid_generic_operand = compile(
        "generic-operand.ts",
        "const value = factory<string>!;",
        CompilerOptions {
            no_check: true,
            ..options("es2015")
        },
    );
    assert_eq!(codes(&invalid_generic_operand), [1109]);
    assert_eq!(
        invalid_generic_operand.semantic_completion,
        SemanticCompletion::Deferred
    );

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
fn ordinary_literal_families_share_expression_hosts_comments_and_root_order() {
    let literal_source = concat!(
        "// ordinary literal expressions\n",
        "const template = (`plain`);\n",
        "const alias = template;\n",
        "((`nested`));\n",
        "(/x+/gi);\n",
        "(\"\\u{67}\");\n",
        "(1_000);",
    );
    let plain = SourceInput::new("plain.ts", Arc::<str>::from("const sibling = 1;"));
    let literals = SourceInput::new("literals.ts", Arc::<str>::from(literal_source));

    for no_check in [false, true] {
        for roots in [
            vec![literals.clone(), plain.clone()],
            vec![plain.clone(), literals.clone()],
        ] {
            let output = Compiler::new().compile(
                roots,
                &CompilerOptions {
                    no_check,
                    ..options("es2015")
                },
            );
            assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
            assert_eq!(output.exit_status, CompileExitStatus::Success);
            assert!(output.diagnostics.is_empty());
            assert_eq!(output.emitted_files.len(), 2);
            assert_eq!(output.emitted_files[0].path, PathBuf::from("literals.js"));
            assert_eq!(
                output.emitted_files[0].text,
                concat!(
                    "\"use strict\";\n",
                    "// ordinary literal expressions\n",
                    "const template = (`plain`);\n",
                    "const alias = template;\n",
                    "((`nested`));\n",
                    "(/x+/gi);\n",
                    "(\"\\u{67}\");\n",
                    "(1000);\n",
                )
            );
            assert_eq!(output.emitted_files[1].path, PathBuf::from("plain.js"));
            assert_eq!(
                output.emitted_files[1].text,
                "\"use strict\";\nconst sibling = 1;\n"
            );
        }
    }

    let mapped = compile(
        "mapped.ts",
        literal_source,
        CompilerOptions {
            source_map: true,
            ..options("es2015")
        },
    );
    assert_eq!(mapped.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(mapped.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(mapped.diagnostics.is_empty());
    assert!(mapped.emitted_files.is_empty());
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
