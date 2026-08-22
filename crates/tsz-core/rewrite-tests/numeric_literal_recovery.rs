use std::path::PathBuf;
use std::sync::Arc;

use tsz::diagnostics::Diagnostic;
use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    Expression, ExpressionKind, Literal, NumberLiteral, Statement, StatementKind, TokenKind,
    parse_source, scan_source,
};
use tsz::{
    CompileExitStatus, CompileOutput, Compiler, CompilerOptions, SemanticCompletion, SourceInput,
};

const OCTAL_1: &str = "Octal literals are not allowed. Use the syntax '0o1'.";
const OCTAL_NEGATIVE_3: &str = "Octal literals are not allowed. Use the syntax '-0o3'.";
const OCTAL_3: &str = "Octal literals are not allowed. Use the syntax '0o3'.";
const LEADING_ZERO: &str = "Decimals with leading zeros are not allowed.";
const DIGIT_EXPECTED: &str = "Digit expected.";
const SEMICOLON_EXPECTED: &str = "';' expected.";

#[derive(Clone, Copy)]
struct ExpectedDiagnostic {
    code: u32,
    start: u32,
    length: u32,
    message: &'static str,
}

#[derive(Clone, Copy)]
struct ExpectedNumber {
    recovery: bool,
    raw: &'static str,
    semantic: &'static str,
    emit: &'static str,
}

struct Case {
    name: &'static str,
    source: &'static str,
    scanner_diagnostics: &'static [ExpectedDiagnostic],
    parser_diagnostics: &'static [ExpectedDiagnostic],
    tokens: &'static [(TokenKind, u32, u32)],
    numbers: &'static [ExpectedNumber],
    javascript: &'static str,
}

const OCTAL_1_DIAGNOSTIC: ExpectedDiagnostic = ExpectedDiagnostic {
    code: 1121,
    start: 0,
    length: 2,
    message: OCTAL_1,
};
const EXPONENT_2_DIAGNOSTIC: ExpectedDiagnostic = ExpectedDiagnostic {
    code: 1124,
    start: 2,
    length: 0,
    message: DIGIT_EXPECTED,
};
const EXPONENT_3_DIAGNOSTIC: ExpectedDiagnostic = ExpectedDiagnostic {
    code: 1124,
    start: 3,
    length: 0,
    message: DIGIT_EXPECTED,
};
const SEMICOLON_DIAGNOSTIC: ExpectedDiagnostic = ExpectedDiagnostic {
    code: 1005,
    start: 2,
    length: 2,
    message: SEMICOLON_EXPECTED,
};

const PLAIN_ZERO: ExpectedNumber = ExpectedNumber {
    recovery: false,
    raw: "0",
    semantic: "0",
    emit: "0",
};
const LEGACY_ONE: ExpectedNumber = ExpectedNumber {
    recovery: true,
    raw: "01",
    semantic: "1",
    emit: "1",
};
const DOT_ZERO: ExpectedNumber = ExpectedNumber {
    recovery: false,
    raw: ".0",
    semantic: ".0",
    emit: ".0",
};
const MISSING_E: ExpectedNumber = ExpectedNumber {
    recovery: true,
    raw: "1e",
    semantic: "1",
    emit: "1e",
};
const COMPLETE_E: ExpectedNumber = ExpectedNumber {
    recovery: false,
    raw: "1e0",
    semantic: "1e0",
    emit: "1e0",
};
const MISSING_E_PLUS: ExpectedNumber = ExpectedNumber {
    recovery: true,
    raw: "1e+",
    semantic: "1",
    emit: "1e+",
};
const COMPLETE_E_PLUS: ExpectedNumber = ExpectedNumber {
    recovery: false,
    raw: "1e+0",
    semantic: "1e+0",
    emit: "1e+0",
};
const LEGACY_THREE: ExpectedNumber = ExpectedNumber {
    recovery: true,
    raw: "03",
    semantic: "3",
    emit: "3",
};
const LEADING_NINE: ExpectedNumber = ExpectedNumber {
    recovery: true,
    raw: "009",
    semantic: "9",
    emit: "9",
};

fn cases() -> Vec<Case> {
    let zero = || Case {
        name: "",
        source: "0",
        scanner_diagnostics: &[],
        parser_diagnostics: &[],
        tokens: &[
            (TokenKind::NumericLiteral, 0, 1),
            (TokenKind::EndOfFile, 1, 1),
        ],
        numbers: &[PLAIN_ZERO],
        javascript: "\"use strict\";\n0;\n",
    };
    let octal = || Case {
        name: "",
        source: "01",
        scanner_diagnostics: &[OCTAL_1_DIAGNOSTIC],
        parser_diagnostics: &[OCTAL_1_DIAGNOSTIC],
        tokens: &[
            (TokenKind::NumericLiteral, 0, 2),
            (TokenKind::EndOfFile, 2, 2),
        ],
        numbers: &[LEGACY_ONE],
        javascript: "\"use strict\";\n1;\n",
    };
    let split = || Case {
        name: "",
        source: "01.0",
        scanner_diagnostics: &[OCTAL_1_DIAGNOSTIC],
        parser_diagnostics: &[OCTAL_1_DIAGNOSTIC, SEMICOLON_DIAGNOSTIC],
        tokens: &[
            (TokenKind::NumericLiteral, 0, 2),
            (TokenKind::NumericLiteral, 2, 4),
            (TokenKind::EndOfFile, 4, 4),
        ],
        numbers: &[LEGACY_ONE, DOT_ZERO],
        javascript: "\"use strict\";\n1;\n.0;\n",
    };
    let missing_e = || Case {
        name: "",
        source: "1e",
        scanner_diagnostics: &[EXPONENT_2_DIAGNOSTIC],
        parser_diagnostics: &[EXPONENT_2_DIAGNOSTIC],
        tokens: &[
            (TokenKind::NumericLiteral, 0, 2),
            (TokenKind::EndOfFile, 2, 2),
        ],
        numbers: &[MISSING_E],
        javascript: "\"use strict\";\n1e;\n",
    };
    let complete_e = || Case {
        name: "",
        source: "1e0",
        scanner_diagnostics: &[],
        parser_diagnostics: &[],
        tokens: &[
            (TokenKind::NumericLiteral, 0, 3),
            (TokenKind::EndOfFile, 3, 3),
        ],
        numbers: &[COMPLETE_E],
        javascript: "\"use strict\";\n1e0;\n",
    };
    let missing_e_plus = || Case {
        name: "",
        source: "1e+",
        scanner_diagnostics: &[EXPONENT_3_DIAGNOSTIC],
        parser_diagnostics: &[EXPONENT_3_DIAGNOSTIC],
        tokens: &[
            (TokenKind::NumericLiteral, 0, 3),
            (TokenKind::EndOfFile, 3, 3),
        ],
        numbers: &[MISSING_E_PLUS],
        javascript: "\"use strict\";\n1e+;\n",
    };
    let complete_e_plus = || Case {
        name: "",
        source: "1e+0",
        scanner_diagnostics: &[],
        parser_diagnostics: &[],
        tokens: &[
            (TokenKind::NumericLiteral, 0, 4),
            (TokenKind::EndOfFile, 4, 4),
        ],
        numbers: &[COMPLETE_E_PLUS],
        javascript: "\"use strict\";\n1e+0;\n",
    };
    let negative = Case {
        name: "scannerNumericLiteral8",
        source: "-03",
        scanner_diagnostics: &[ExpectedDiagnostic {
            code: 1121,
            start: 0,
            length: 3,
            message: OCTAL_NEGATIVE_3,
        }],
        parser_diagnostics: &[ExpectedDiagnostic {
            code: 1121,
            start: 0,
            length: 3,
            message: OCTAL_NEGATIVE_3,
        }],
        tokens: &[
            (TokenKind::Minus, 0, 1),
            (TokenKind::NumericLiteral, 1, 3),
            (TokenKind::EndOfFile, 3, 3),
        ],
        numbers: &[LEGACY_THREE],
        javascript: "\"use strict\";\n-3;\n",
    };
    let leading = Case {
        name: "scannerNumericLiteral9",
        source: "009",
        scanner_diagnostics: &[ExpectedDiagnostic {
            code: 1489,
            start: 0,
            length: 3,
            message: LEADING_ZERO,
        }],
        parser_diagnostics: &[ExpectedDiagnostic {
            code: 1489,
            start: 0,
            length: 3,
            message: LEADING_ZERO,
        }],
        tokens: &[
            (TokenKind::NumericLiteral, 0, 3),
            (TokenKind::EndOfFile, 3, 3),
        ],
        numbers: &[LEADING_NINE],
        javascript: "\"use strict\";\n9;\n",
    };

    let mut rows = vec![
        zero(),
        octal(),
        split(),
        missing_e(),
        complete_e(),
        missing_e_plus(),
        complete_e_plus(),
        zero(),
        octal(),
        split(),
        missing_e(),
        complete_e(),
        missing_e_plus(),
        complete_e_plus(),
        negative,
        leading,
    ];
    let names = [
        "scannerES3NumericLiteral1",
        "scannerES3NumericLiteral2",
        "scannerES3NumericLiteral3",
        "scannerES3NumericLiteral4",
        "scannerES3NumericLiteral5",
        "scannerES3NumericLiteral6",
        "scannerES3NumericLiteral7",
        "scannerNumericLiteral1",
        "scannerNumericLiteral2",
        "scannerNumericLiteral3",
        "scannerNumericLiteral4",
        "scannerNumericLiteral5",
        "scannerNumericLiteral6",
        "scannerNumericLiteral7",
        "scannerNumericLiteral8",
        "scannerNumericLiteral9",
    ];
    for (row, name) in rows.iter_mut().zip(names) {
        row.name = name;
    }
    rows
}

fn options(no_check: bool, no_emit: bool) -> CompilerOptions {
    CompilerOptions {
        no_check,
        no_emit,
        target: "es2015".to_string(),
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

fn diagnostic_facts(diagnostics: &[Diagnostic]) -> Vec<(u32, u32, u32, &str)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            )
        })
        .collect()
}

fn expected_diagnostic_facts(diagnostics: &[ExpectedDiagnostic]) -> Vec<(u32, u32, u32, &str)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message,
            )
        })
        .collect()
}

fn collect_numbers(statements: &[Statement]) -> Vec<&NumberLiteral> {
    statements
        .iter()
        .filter_map(|statement| match &statement.kind {
            StatementKind::Expression(expression) => number_in_expression(expression),
            _ => None,
        })
        .collect()
}

fn number_in_expression(expression: &Expression) -> Option<&NumberLiteral> {
    match &expression.kind {
        ExpressionKind::Literal(Literal::Number(number)) => Some(number),
        ExpressionKind::Unary { operand, .. } => number_in_expression(operand),
        _ => None,
    }
}

fn javascript(output: &CompileOutput) -> &str {
    output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("expected JavaScript product")
        .text
        .as_str()
}

fn assert_incomplete(path: &str, source: &str, options: CompilerOptions) {
    let output = compile(path, source, options);
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Deferred,
        "{path}: {source:?}: {:?}",
        output.diagnostics
    );
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(output.emitted_files.is_empty(), "{path}: {source:?}");
}

#[test]
fn exact_sixteen_row_scanner_parser_and_ast_manifest() {
    let cases = cases();
    assert_eq!(cases.len(), 16);
    for case in cases {
        let source = SourceText::new(
            FileId(7),
            PathBuf::from(format!("{}.ts", case.name)),
            Arc::<str>::from(case.source),
        );
        let scanned = scan_source(&source);
        assert_eq!(
            diagnostic_facts(&scanned.diagnostics),
            expected_diagnostic_facts(case.scanner_diagnostics),
            "{} scanner diagnostics",
            case.name
        );
        let tokens = scanned
            .tokens
            .iter()
            .map(|token| (token.kind, token.span.start, token.span.end))
            .collect::<Vec<_>>();
        assert_eq!(tokens, case.tokens, "{} tokenization", case.name);

        let parsed = parse_source(&source);
        assert_eq!(
            diagnostic_facts(&parsed.diagnostics),
            expected_diagnostic_facts(case.parser_diagnostics),
            "{} parser diagnostics",
            case.name
        );
        let numbers = collect_numbers(&parsed.unit.statements);
        assert_eq!(numbers.len(), case.numbers.len(), "{} AST", case.name);
        for (number, expected) in numbers.into_iter().zip(case.numbers) {
            assert_eq!(
                matches!(number, NumberLiteral::Recovery(_)),
                expected.recovery,
                "{} recovery kind",
                case.name
            );
            assert_eq!(number.raw(), expected.raw, "{} raw", case.name);
            assert_eq!(
                number.semantic_text(),
                expected.semantic,
                "{} semantic",
                case.name
            );
            assert_eq!(number.emit_text(false), expected.emit, "{} emit", case.name);
        }
    }
}

#[test]
fn exact_sixteen_row_checked_no_check_no_emit_and_javascript_manifest() {
    for case in cases() {
        for no_check in [false, true] {
            for no_emit in [false, true] {
                let output = compile(
                    &format!("{}.ts", case.name),
                    case.source,
                    options(no_check, no_emit),
                );
                assert_eq!(
                    output.semantic_completion,
                    SemanticCompletion::Complete,
                    "{} noCheck={no_check} noEmit={no_emit}: {:?}",
                    case.name,
                    output.diagnostics
                );
                assert_eq!(
                    diagnostic_facts(&output.diagnostics),
                    expected_diagnostic_facts(case.parser_diagnostics),
                    "{} noCheck={no_check} noEmit={no_emit}",
                    case.name
                );
                let expected_status = if case.parser_diagnostics.is_empty() {
                    CompileExitStatus::Success
                } else if no_emit {
                    CompileExitStatus::DiagnosticsPresentOutputsSkipped
                } else {
                    CompileExitStatus::DiagnosticsPresentOutputsGenerated
                };
                assert_eq!(output.exit_status, expected_status, "{}", case.name);
                assert_eq!(output.emitted_files.is_empty(), no_emit, "{}", case.name);
                if !no_emit {
                    assert_eq!(javascript(&output), case.javascript, "{}", case.name);
                }
            }
        }
    }
}

#[test]
fn previous_significant_minus_selects_the_exact_ts1121_span_and_replacement() {
    let cases = [
        ("-03", 0, 3, OCTAL_NEGATIVE_3),
        ("- 03", 1, 3, OCTAL_NEGATIVE_3),
        ("-\t03", 1, 3, OCTAL_NEGATIVE_3),
        ("-/*c*/03", 5, 3, OCTAL_NEGATIVE_3),
        ("-\n03", 1, 3, OCTAL_NEGATIVE_3),
        ("-\r\n03", 2, 3, OCTAL_NEGATIVE_3),
        ("(-03)", 1, 3, OCTAL_NEGATIVE_3),
        ("-(03)", 2, 2, OCTAL_3),
        ("+-03", 1, 3, OCTAL_NEGATIVE_3),
        ("--03", 2, 2, OCTAL_3),
        ("-+03", 2, 2, OCTAL_3),
        ("a-03", 1, 3, OCTAL_NEGATIVE_3),
        ("3-03", 1, 3, OCTAL_NEGATIVE_3),
        ("-;03", 2, 2, OCTAL_3),
        ("a -= 03", 5, 2, OCTAL_3),
    ];
    for (text, start, length, message) in cases {
        let source = SourceText::new(FileId(11), PathBuf::from("sign.ts"), Arc::<str>::from(text));
        let scanned = scan_source(&source);
        let diagnostic = scanned
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == 1121)
            .unwrap_or_else(|| panic!("missing TS1121 for {text:?}: {:?}", scanned.diagnostics));
        assert_eq!(
            (
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ),
            (start, length, message),
            "{text:?}"
        );
    }

    let saturated = "07777777777777777777777";
    let source = SourceText::new(
        FileId(11),
        PathBuf::from("saturated.ts"),
        Arc::<str>::from(saturated),
    );
    let scanned = scan_source(&source);
    assert_eq!(
        scanned.diagnostics[0].message_text,
        "Octal literals are not allowed. Use the syntax '0o777777777777777777777'."
    );
}

#[test]
fn legacy_octal_terminates_before_suffixes_and_parser_asi_is_line_sensitive() {
    struct Adjacency<'a> {
        source: &'a str,
        tokens: &'a [(TokenKind, u32, u32)],
        diagnostics: &'a [(u32, u32, u32)],
    }
    let cases = [
        Adjacency {
            source: "01_2",
            tokens: &[
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::Identifier, 2, 4),
                (TokenKind::EndOfFile, 4, 4),
            ],
            diagnostics: &[(1121, 0, 2), (1005, 2, 2)],
        },
        Adjacency {
            source: "08_0",
            tokens: &[
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::Identifier, 2, 4),
                (TokenKind::EndOfFile, 4, 4),
            ],
            diagnostics: &[(1489, 0, 2), (1005, 2, 2)],
        },
        Adjacency {
            source: "01e",
            tokens: &[
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::Identifier, 2, 3),
                (TokenKind::EndOfFile, 3, 3),
            ],
            diagnostics: &[(1121, 0, 2), (1005, 2, 1)],
        },
        Adjacency {
            source: "01n",
            tokens: &[
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::Identifier, 2, 3),
                (TokenKind::EndOfFile, 3, 3),
            ],
            diagnostics: &[(1121, 0, 2), (1005, 2, 1)],
        },
        Adjacency {
            source: "01foo",
            tokens: &[
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::Identifier, 2, 5),
                (TokenKind::EndOfFile, 5, 5),
            ],
            diagnostics: &[(1121, 0, 2), (1005, 2, 3)],
        },
        Adjacency {
            source: "01 .0",
            tokens: &[
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::NumericLiteral, 3, 5),
                (TokenKind::EndOfFile, 5, 5),
            ],
            diagnostics: &[(1121, 0, 2), (1005, 3, 2)],
        },
        Adjacency {
            source: "01\n.0",
            tokens: &[
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::NumericLiteral, 3, 5),
                (TokenKind::EndOfFile, 5, 5),
            ],
            diagnostics: &[(1121, 0, 2)],
        },
        Adjacency {
            source: "01/*c*/.0",
            tokens: &[
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::NumericLiteral, 7, 9),
                (TokenKind::EndOfFile, 9, 9),
            ],
            diagnostics: &[(1121, 0, 2), (1005, 7, 2)],
        },
        Adjacency {
            source: "01//c\n.0",
            tokens: &[
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::NumericLiteral, 6, 8),
                (TokenKind::EndOfFile, 8, 8),
            ],
            diagnostics: &[(1121, 0, 2)],
        },
    ];
    for case in cases {
        let source = SourceText::new(
            FileId(12),
            PathBuf::from("adjacent.ts"),
            Arc::<str>::from(case.source),
        );
        let scanned = scan_source(&source);
        assert_eq!(
            scanned
                .tokens
                .iter()
                .map(|token| (token.kind, token.span.start, token.span.end))
                .collect::<Vec<_>>(),
            case.tokens,
            "{:?}",
            case.source
        );
        let parsed = parse_source(&source);
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
                .collect::<Vec<_>>(),
            case.diagnostics,
            "{:?}",
            case.source
        );
        assert_incomplete("adjacent.ts", case.source, options(false, false));
    }
}

#[test]
fn separator_radix_and_fraction_adjacencies_fail_closed() {
    for source in [
        "0e_0", "0e+_0", "0_1", "1__0", "1_", "1_.0", "1._0", "1e_2", "1e+_2", "0x_FF", "0b_1",
        "0o_1", "0x", "0b", "0o", ".1e", "1.e", "1_n", "0_1n", "1__0n",
    ] {
        for no_check in [false, true] {
            assert_incomplete("separator.ts", source, options(no_check, false));
        }
    }

    // Decimal invalid-separator runs keep an immediate BigInt suffix in the
    // same token, matching the pre-campaign scanner boundary. TS6188/TS6189
    // remain unowned, so every product mode stays explicitly Deferred.
    for (source_text, end) in [("1_n", 3), ("0_1n", 4), ("1__0n", 5)] {
        let source = SourceText::new(
            FileId(15),
            PathBuf::from("invalid-separator-bigint.ts"),
            Arc::<str>::from(source_text),
        );
        let scanned = scan_source(&source);
        assert!(scanned.diagnostics.is_empty(), "{source_text:?}");
        assert_eq!(
            scanned
                .tokens
                .iter()
                .map(|token| (token.kind, token.span.start, token.span.end))
                .collect::<Vec<_>>(),
            [
                (TokenKind::BigIntLiteral, 0, end),
                (TokenKind::EndOfFile, end, end),
            ],
            "{source_text:?}"
        );
        let parsed = parse_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{source_text:?}");
        let [
            Statement {
                kind:
                    StatementKind::Expression(Expression {
                        kind: ExpressionKind::Literal(Literal::BigInt(raw)),
                        ..
                    }),
                ..
            },
        ] = parsed.unit.statements.as_slice()
        else {
            panic!("{source_text:?} should remain one BigInt expression");
        };
        assert_eq!(raw, source_text);

        for no_check in [false, true] {
            let output = compile(
                "invalid-separator-bigint.ts",
                source_text,
                options(no_check, false),
            );
            assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
            assert_eq!(
                output.stats.semantic_completion,
                SemanticCompletion::Deferred
            );
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
            assert!(output.diagnostics.is_empty(), "{source_text:?}");
            assert!(output.emitted_files.is_empty(), "{source_text:?}");
        }
    }

    // Empty radix prefixes keep an immediate BigInt suffix in the recovery
    // token. Exact missing-radix diagnostics remain outside this campaign.
    for source_text in ["0xn", "0bn", "0on"] {
        let source = SourceText::new(
            FileId(16),
            PathBuf::from("empty-radix-bigint.ts"),
            Arc::<str>::from(source_text),
        );
        let scanned = scan_source(&source);
        assert!(scanned.diagnostics.is_empty(), "{source_text:?}");
        assert_eq!(
            scanned
                .tokens
                .iter()
                .map(|token| (token.kind, token.span.start, token.span.end))
                .collect::<Vec<_>>(),
            [
                (TokenKind::BigIntLiteral, 0, 3),
                (TokenKind::EndOfFile, 3, 3),
            ],
            "{source_text:?}"
        );
        let parsed = parse_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{source_text:?}");
        let [
            Statement {
                kind:
                    StatementKind::Expression(Expression {
                        kind: ExpressionKind::Literal(Literal::BigInt(raw)),
                        ..
                    }),
                ..
            },
        ] = parsed.unit.statements.as_slice()
        else {
            panic!("{source_text:?} should remain one BigInt expression");
        };
        assert_eq!(raw, source_text);
        for no_check in [false, true] {
            let output = compile(
                "empty-radix-bigint.ts",
                source_text,
                options(no_check, false),
            );
            assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
            assert_eq!(
                output.stats.semantic_completion,
                SemanticCompletion::Deferred
            );
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
            assert!(output.diagnostics.is_empty(), "{source_text:?}");
            assert!(output.emitted_files.is_empty(), "{source_text:?}");
        }
    }

    // An immediate `n` after an exponent with no digits stays in the Numeric
    // recovery token. TS1352 is deliberately not partially modeled here.
    for (source_text, end, diagnostic_start, semantic_text) in
        [("3en", 3, 2, "3"), ("1e+n", 4, 3, "1")]
    {
        let source = SourceText::new(
            FileId(17),
            PathBuf::from("missing-exponent-n.ts"),
            Arc::<str>::from(source_text),
        );
        let scanned = scan_source(&source);
        assert_eq!(
            diagnostic_facts(&scanned.diagnostics),
            [(1124, diagnostic_start, 0, DIGIT_EXPECTED)],
            "{source_text:?}"
        );
        assert_eq!(
            scanned
                .tokens
                .iter()
                .map(|token| (token.kind, token.span.start, token.span.end))
                .collect::<Vec<_>>(),
            [
                (TokenKind::NumericLiteral, 0, end),
                (TokenKind::EndOfFile, end, end),
            ],
            "{source_text:?}"
        );
        let parsed = parse_source(&source);
        assert_eq!(
            diagnostic_facts(&parsed.diagnostics),
            [(1124, diagnostic_start, 0, DIGIT_EXPECTED)],
            "{source_text:?}"
        );
        let numbers = collect_numbers(&parsed.unit.statements);
        let [number] = numbers.as_slice() else {
            panic!("{source_text:?} should remain one recovery number");
        };
        assert!(matches!(number, NumberLiteral::Recovery(_)));
        assert_eq!(number.raw(), source_text);
        assert_eq!(number.semantic_text(), semantic_text);
        assert!(!number.validation_supported());
        for no_check in [false, true] {
            let output = compile(
                "missing-exponent-n.ts",
                source_text,
                options(no_check, false),
            );
            assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
            assert_eq!(
                output.stats.semantic_completion,
                SemanticCompletion::Deferred
            );
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
            assert_eq!(
                diagnostic_facts(&output.diagnostics),
                [(1124, diagnostic_start, 0, DIGIT_EXPECTED)],
                "{source_text:?}"
            );
            assert!(output.emitted_files.is_empty(), "{source_text:?}");
        }
    }

    // `n` remains the start of an identifier when an IdentifierPart follows.
    // The incomplete numeric source then fails closed before name checking.
    for (source_text, numeric_end, identifier_end, diagnostic_start) in [
        ("3enx", 2, 4, 2),
        ("3en_", 2, 4, 2),
        ("3en$", 2, 4, 2),
        ("3en0", 2, 4, 2),
        ("1e+nx", 3, 5, 3),
    ] {
        let source = SourceText::new(
            FileId(18),
            PathBuf::from("missing-exponent-identifier.ts"),
            Arc::<str>::from(source_text),
        );
        let scanned = scan_source(&source);
        assert_eq!(
            diagnostic_facts(&scanned.diagnostics),
            [(1124, diagnostic_start, 0, DIGIT_EXPECTED)],
            "{source_text:?}"
        );
        assert_eq!(
            scanned
                .tokens
                .iter()
                .map(|token| (token.kind, token.span.start, token.span.end))
                .collect::<Vec<_>>(),
            [
                (TokenKind::NumericLiteral, 0, numeric_end),
                (TokenKind::Identifier, numeric_end, identifier_end),
                (TokenKind::EndOfFile, identifier_end, identifier_end),
            ],
            "{source_text:?}"
        );
        for no_check in [false, true] {
            let output = compile(
                "missing-exponent-identifier.ts",
                source_text,
                options(no_check, false),
            );
            assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
            assert_eq!(output.stats.types, 0);
            assert!(
                output
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == 1124)
            );
            assert!(
                output
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.code != 2304)
            );
            assert!(output.emitted_files.is_empty());
        }
    }

    for no_check in [false, true] {
        let output = compile("identifier-tail.ts", "1efoo", options(no_check, false));
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.stats.types, 0);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == 1124)
        );
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != 2304)
        );
        assert!(output.emitted_files.is_empty());
    }

    // TS6188 and its exact separator spans are not owned by this campaign.
    // These typed scanner facts must defer without inventing a partial event.
    for source_text in ["0_1", "0x_FF", "0b_1", "0o_1"] {
        let source = SourceText::new(
            FileId(13),
            PathBuf::from("unowned-separator.ts"),
            Arc::<str>::from(source_text),
        );
        let scanned = scan_source(&source);
        assert!(scanned.diagnostics.is_empty(), "{source_text:?}");
        let parsed = parse_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{source_text:?}");
        let output = compile("unowned-separator.ts", source_text, options(false, false));
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(output.diagnostics.is_empty(), "{source_text:?}");
        assert!(output.emitted_files.is_empty(), "{source_text:?}");
    }

    // Empty radix prefixes are outside this campaign's exact diagnostic and
    // parser-recovery surface. Keep the whole prefix in one typed recovery
    // token so the deferred product cannot cascade through an invented name.
    for source_text in ["0x", "0b", "0o"] {
        let source = SourceText::new(
            FileId(13),
            PathBuf::from("empty-radix.ts"),
            Arc::<str>::from(source_text),
        );
        let scanned = scan_source(&source);
        assert!(scanned.diagnostics.is_empty(), "{source_text:?}");
        assert_eq!(
            scanned
                .tokens
                .iter()
                .map(|token| (token.kind, token.span.start, token.span.end))
                .collect::<Vec<_>>(),
            [
                (TokenKind::NumericLiteral, 0, 2),
                (TokenKind::EndOfFile, 2, 2),
            ],
            "{source_text:?}"
        );
        let parsed = parse_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{source_text:?}");
        let numbers = collect_numbers(&parsed.unit.statements);
        let [number] = numbers.as_slice() else {
            panic!("{source_text:?} should remain one recovery number");
        };
        assert!(matches!(number, NumberLiteral::Recovery(_)));
        assert_eq!(number.raw(), source_text);
        let output = compile("empty-radix.ts", source_text, options(false, false));
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(output.diagnostics.is_empty(), "{source_text:?}");
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != 2304)
        );
    }

    // Valid separators remain distinct from scanner recovery and are staged
    // through their own exact raw/canonical syntax representation.
    for source_text in ["1_000", "0xF_F", "0b1_0", "0o7_0"] {
        let source = SourceText::new(
            FileId(14),
            PathBuf::from("valid-separator.ts"),
            Arc::<str>::from(source_text),
        );
        let scanned = scan_source(&source);
        assert!(scanned.diagnostics.is_empty(), "{source_text:?}");
        let parsed = parse_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{source_text:?}");
        let numbers = collect_numbers(&parsed.unit.statements);
        let [number] = numbers.as_slice() else {
            panic!("{source_text:?} should remain one separated number");
        };
        assert!(matches!(number, NumberLiteral::Separated(_)));
        assert_eq!(number.raw(), source_text);
    }
}

#[test]
fn unsupported_hosts_sources_and_service_display_remain_deferred() {
    for source in [
        "const value = 01;",
        "let value = 01;",
        "var value = 01;",
        "const value: 1 = 01;",
        "type Value = 01;",
        "(01);",
        "[01];",
        "({ value: 01 });",
        "consume(01);",
        "01 + 1;",
        "function f() { 01; }",
        "class C { value = 01; }",
        "01; 03;",
        "0; 01;",
        "/* comment */01;",
    ] {
        for no_check in [false, true] {
            assert_incomplete("host.ts", source, options(no_check, false));
        }
    }
    for path in [
        "value.js",
        "value.jsx",
        "value.tsx",
        "value.mts",
        "value.cts",
        "value.d.ts",
        "value.TS",
    ] {
        assert_incomplete(path, "01;", options(false, false));
    }

    let mut service = LanguageService::new(options(false, false));
    service.open("service.ts", Arc::<str>::from("const renamed = 01;"));
    assert!(service.quick_info("service.ts", 7).is_none());
    let output = service.compile();
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let relation = compile(
        "relation.ts",
        "const renamed: 2 = 01;",
        options(false, false),
    );
    assert_eq!(relation.semantic_completion, SemanticCompletion::Deferred);
    assert!(
        relation
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == 1121)
    );
    assert!(
        relation
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2322)
    );
}

#[test]
fn unvalidated_recovery_is_deferred_across_repeat_and_root_order() {
    let roots = || {
        vec![
            SourceInput::new("a.ts", Arc::<str>::from("0e_0;")),
            SourceInput::new("b.ts", Arc::<str>::from("1e_;")),
            SourceInput::new("c.ts", Arc::<str>::from(".1e;")),
        ]
    };
    let compiler = Compiler::new();
    for no_check in [false, true] {
        let baseline = compiler.compile(roots(), &options(no_check, false));
        assert_eq!(baseline.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(
            baseline.stats.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert_eq!(baseline.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(baseline.emitted_files.is_empty());
        assert_eq!(
            baseline
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.file.as_str(),
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.message_text.as_str(),
                ))
                .collect::<Vec<_>>(),
            [
                ("b.ts", 1124, 3, 0, DIGIT_EXPECTED),
                ("c.ts", 1124, 3, 0, DIGIT_EXPECTED),
            ]
        );
        let expected_diagnostics = baseline.diagnostics;
        let expected_type_count = baseline.stats.types;
        for iteration in 0..6 {
            let mut inputs = roots();
            if iteration % 2 == 1 {
                inputs.reverse();
            }
            let output = compiler.compile(inputs, &options(no_check, false));
            assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
            assert_eq!(
                output.stats.semantic_completion,
                SemanticCompletion::Deferred
            );
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
            assert_eq!(output.diagnostics, expected_diagnostics);
            assert_eq!(output.stats.types, expected_type_count);
            assert!(output.emitted_files.is_empty());
        }
    }
}

#[test]
fn exact_option_root_order_repeat_and_existing_literal_gates_are_stable() {
    for target in ["es6", "ES6", "es2015", "ES2015"] {
        for module in ["commonjs", "esnext", "preserve"] {
            for no_check in [false, true] {
                for no_emit in [false, true] {
                    let mut supported = options(no_check, no_emit);
                    supported.target = target.to_string();
                    supported.module = module.to_string();
                    let output = compile("owned.ts", "01;", supported);
                    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
                    assert_eq!(output.emitted_files.is_empty(), no_emit);
                }
            }
        }
    }

    let mut rejected = Vec::new();
    for target in ["es5", "es2016", "esnext", " es6", "es6 "] {
        let mut candidate = options(false, false);
        candidate.target = target.to_string();
        rejected.push(candidate);
    }
    let mut unsupported_module = options(false, false);
    unsupported_module.module = "amd".to_string();
    rejected.push(unsupported_module);
    let mut explicit_lib = options(false, false);
    explicit_lib.lib = Some(vec!["es2015".to_string()]);
    rejected.push(explicit_lib);
    macro_rules! reject_bool {
        ($field:ident) => {{
            let mut candidate = options(false, false);
            candidate.$field = true;
            rejected.push(candidate);
        }};
    }
    reject_bool!(no_lib);
    reject_bool!(no_emit_on_error);
    reject_bool!(declaration);
    reject_bool!(declaration_map);
    reject_bool!(source_map);
    reject_bool!(inline_source_map);
    reject_bool!(remove_comments);
    for field in ["root", "out", "declaration"] {
        let mut candidate = options(false, false);
        match field {
            "root" => candidate.root_dir = Some(PathBuf::from("root")),
            "out" => candidate.out_dir = Some(PathBuf::from("out")),
            "declaration" => candidate.declaration_dir = Some(PathBuf::from("types")),
            _ => unreachable!(),
        }
        rejected.push(candidate);
    }
    for candidate in rejected {
        assert_incomplete("owned.ts", "01;", candidate);
    }

    let roots = || {
        vec![
            SourceInput::new("a.ts", Arc::<str>::from("01;")),
            SourceInput::new("b.ts", Arc::<str>::from("03;")),
        ]
    };
    let compiler = Compiler::new();
    let expected = compiler.compile(roots(), &options(false, false));
    assert_eq!(expected.semantic_completion, SemanticCompletion::Complete);
    let expected_diagnostics = expected.diagnostics.clone();
    let expected_emit = expected
        .emitted_files
        .iter()
        .map(|file| (file.path.clone(), file.text.clone(), file.declaration))
        .collect::<Vec<_>>();
    for iteration in 0..10 {
        let mut inputs = roots();
        if iteration % 2 == 1 {
            inputs.reverse();
        }
        let output = compiler.compile(inputs, &options(false, false));
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(output.diagnostics, expected_diagnostics);
        assert_eq!(
            output
                .emitted_files
                .iter()
                .map(|file| (file.path.clone(), file.text.clone(), file.declaration))
                .collect::<Vec<_>>(),
            expected_emit
        );
    }
    assert_incomplete("mixed-entry.ts", "01; 009;", options(false, false));
    let mixed_roots = compiler.compile(
        vec![
            SourceInput::new("a.ts", Arc::<str>::from("01;")),
            SourceInput::new("b.ts", Arc::<str>::from("009;")),
        ],
        &options(false, false),
    );
    assert_eq!(
        mixed_roots.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(mixed_roots.emitted_files.is_empty());

    for source in ["`plain`;", r#"var value = "\u{67}";"#, "/a/g;"] {
        let output = compile("existing.ts", source, options(false, false));
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source:?}: {:?}",
            output.diagnostics
        );
        assert!(!output.emitted_files.is_empty(), "{source:?}");
    }
}
