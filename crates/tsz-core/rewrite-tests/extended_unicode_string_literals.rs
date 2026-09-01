use std::path::PathBuf;
use std::sync::Arc;

use tsz::diagnostics::Diagnostic;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    ExpressionKind, Literal, StatementKind, StringLiteral, parse_source, scan_source,
};
use tsz::{
    CompileExitStatus, CompileOutput, Compiler, CompilerOptions, SemanticCompletion, SourceInput,
};

const RANGE: &str = "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.";
const HEX: &str = "Hexadecimal digit expected.";
const UNICODE_EOF: &str = "Unterminated Unicode escape sequence.";
const TEXT_EOF: &str = "Unexpected end of text.";
const STRING_EOF: &str = "Unterminated string literal.";

const ASSERT_COMMENTS: &str = concat!(
    "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
    "//  1. Assert: 0 ≤ cp ≤ 0x10FFFF.\n",
);
const BMP_COMMENTS: &str = concat!(
    "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
    "//  2. If cp ≤ 65535, return cp.\n",
    "// (FFFF == 65535)\n",
);
const ASTRAL_COMMENTS: &str = concat!(
    "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
    "//  2. If cp ≤ 65535, return cp.\n",
    "// (10000 == 65536)\n",
);
const HIGH_SURROGATE_COMMENTS: &str = concat!(
    "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
    "//  2. Let cu1 be floor((cp – 65536) / 1024) + 0xD800.\n",
    "// Although we should just get back a single code point value of 0xD800,\n",
    "// this is a useful edge-case test.\n",
);
const LOW_SURROGATE_COMMENTS: &str = concat!(
    "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
    "//  2. Let cu2 be ((cp – 65536) modulo 1024) + 0xDC00.\n",
    "// Although we should just get back a single code point value of 0xDC00,\n",
    "// this is a useful edge-case test.\n",
);

#[derive(Clone, Copy)]
struct ExpectedDiagnostic {
    code: u32,
    start: u32,
    length: u32,
    message: &'static str,
}

struct Case {
    row: u8,
    comments: &'static str,
    raw: &'static str,
    cooked: Vec<u16>,
    terminated: bool,
    invalid: bool,
    extended: bool,
    diagnostics: Vec<ExpectedDiagnostic>,
}

fn ascii(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn cases() -> Vec<Case> {
    let range = |start, length| ExpectedDiagnostic {
        code: 1198,
        start,
        length,
        message: RANGE,
    };
    let hex = |start| ExpectedDiagnostic {
        code: 1125,
        start,
        length: 0,
        message: HEX,
    };
    let unicode_eof = |start| ExpectedDiagnostic {
        code: 1199,
        start,
        length: 0,
        message: UNICODE_EOF,
    };
    vec![
        case(1, "", r#""\u{0}""#, vec![0x0000]),
        case(2, "", r#""\u{00}""#, vec![0x0000]),
        case(3, "", r#""\u{0000}""#, vec![0x0000]),
        case(4, "", r#""\u{00000000}""#, vec![0x0000]),
        case(
            5,
            "",
            r#""\u{48}\u{65}\u{6c}\u{6c}\u{6f}\u{20}\u{77}\u{6f}\u{72}\u{6c}\u{64}""#,
            ascii("Hello world"),
        ),
        case(6, ASSERT_COMMENTS, r#""\u{10FFFF}""#, vec![0xdbff, 0xdfff]),
        invalid_case(7, ASSERT_COMMENTS, r#""\u{110000}""#, vec![range(105, 6)]),
        case(8, BMP_COMMENTS, r#""\u{FFFF}""#, vec![0xffff]),
        case(9, ASTRAL_COMMENTS, r#""\u{10000}""#, vec![0xd800, 0xdc00]),
        case(10, HIGH_SURROGATE_COMMENTS, r#""\u{D800}""#, vec![0xd800]),
        case(11, LOW_SURROGATE_COMMENTS, r#""\u{DC00}""#, vec![0xdc00]),
        invalid_case(12, "", r#""\u{FFFFFFFF}""#, vec![range(13, 8)]),
        case(13, "", r#""\u{DDDDD}""#, vec![0xdb37, 0xdddd]),
        invalid_case(
            14,
            "// Shouldn't work, negatives are not allowed.\n",
            r#""\u{-DDDD}""#,
            vec![hex(59)],
        ),
        case(
            15,
            "",
            r#""\u{abcd}\u{ef12}\u{3456}\u{7890}""#,
            vec![0xabcd, 0xef12, 0x3456, 0x7890],
        ),
        case(
            16,
            "",
            r#""\u{ABCD}\u{EF12}\u{3456}\u{7890}""#,
            vec![0xabcd, 0xef12, 0x3456, 0x7890],
        ),
        invalid_case(
            17,
            "",
            r#""\u{r}\u{n}\u{t}""#,
            vec![hex(13), hex(18), hex(23)],
        ),
        case(18, "", r#""\u{65}\u{65}""#, vec![0x0065, 0x0065]),
        invalid_case(19, "", r#""\u{}""#, vec![hex(13)]),
        invalid_case(20, "", r#""\u{""#, vec![hex(13)]),
        invalid_case(21, "", r#""\u{67""#, vec![unicode_eof(15)]),
        invalid_case(22, "", r#""\u{00000000000067""#, vec![unicode_eof(27)]),
        case(23, "", r#""\u{00000000000067}""#, vec![0x0067]),
        unterminated_case(
            24,
            r#""\u{00000000000067"#,
            true,
            false,
            vec![ExpectedDiagnostic {
                code: 1126,
                start: 27,
                length: 0,
                message: TEXT_EOF,
            }],
        ),
        unterminated_case(
            25,
            r#""\u{00000000000067}"#,
            false,
            true,
            vec![ExpectedDiagnostic {
                code: 1002,
                start: 28,
                length: 0,
                message: STRING_EOF,
            }],
        ),
    ]
}

const fn case(row: u8, comments: &'static str, raw: &'static str, cooked: Vec<u16>) -> Case {
    Case {
        row,
        comments,
        raw,
        cooked,
        terminated: true,
        invalid: false,
        extended: true,
        diagnostics: Vec::new(),
    }
}

fn invalid_case(
    row: u8,
    comments: &'static str,
    raw: &'static str,
    diagnostics: Vec<ExpectedDiagnostic>,
) -> Case {
    Case {
        cooked: ascii(&raw[1..raw.len() - 1]),
        diagnostics,
        invalid: true,
        extended: false,
        ..case(row, comments, raw, Vec::new())
    }
}

fn unterminated_case(
    row: u8,
    raw: &'static str,
    invalid: bool,
    extended: bool,
    diagnostics: Vec<ExpectedDiagnostic>,
) -> Case {
    Case {
        row,
        comments: "",
        raw,
        cooked: if invalid {
            ascii(&raw[1..])
        } else {
            vec![0x0067]
        },
        terminated: false,
        invalid,
        extended,
        diagnostics,
    }
}

fn source_text(case: &Case, terminator: &str) -> String {
    let semicolon = if case.terminated { ";" } else { "" };
    format!(
        "\n{}var x = {}{semicolon}{terminator}",
        case.comments, case.raw
    )
}

fn diagnostics(output: &[Diagnostic]) -> Vec<(u32, u32, u32, &str)> {
    output
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

fn expected_diagnostics(case: &Case) -> Vec<(u32, u32, u32, &str)> {
    case.diagnostics
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

fn javascript(output: &CompileOutput) -> &str {
    output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("expected JavaScript product")
        .text
        .as_str()
}

#[test]
fn exact_twenty_five_row_syntax_manifest_and_event_ownership() {
    for case in cases() {
        let text = source_text(&case, "");
        let source = SourceText::new(
            FileId(7),
            PathBuf::from(format!("unicodeExtendedEscapesInStrings{:02}.ts", case.row)),
            Arc::<str>::from(text),
        );
        let scanned = scan_source(&source);
        let parsed = parse_source(&source);
        assert_eq!(parsed.diagnostics, scanned.diagnostics, "row {}", case.row);
        assert_eq!(
            diagnostics(&parsed.diagnostics),
            expected_diagnostics(&case),
            "row {}",
            case.row
        );
        let [statement] = parsed.unit.statements.as_slice() else {
            panic!("row {} did not parse as one statement", case.row);
        };
        let StatementKind::Variable(declaration) = &statement.kind else {
            panic!("row {} did not parse as a variable", case.row);
        };
        let Some(initializer) = &declaration.declarators[0].initializer else {
            panic!("row {} lost its initializer", case.row);
        };
        let ExpressionKind::Literal(Literal::String(StringLiteral::Extended(literal))) =
            &initializer.kind
        else {
            panic!("row {} lost extended-string syntax ownership", case.row);
        };
        assert_eq!(literal.raw, case.raw, "row {} raw", case.row);
        assert_eq!(
            literal.cooked.units(),
            case.cooked,
            "row {} cooked",
            case.row
        );
        assert_eq!(
            literal.terminated, case.terminated,
            "row {} termination",
            case.row
        );
        assert_eq!(
            literal.contains_invalid_escape, case.invalid,
            "row {} invalid",
            case.row
        );
        assert_eq!(
            literal.contains_extended_unicode_escape, case.extended,
            "row {} flag",
            case.row
        );
    }
}

#[test]
fn diagnostics_survive_checked_no_check_and_no_emit_while_products_stay_owned() {
    for case in cases() {
        let text = source_text(&case, "");
        for no_check in [false, true] {
            for no_emit in [false, true] {
                let output = compile(
                    &format!("row{:02}.ts", case.row),
                    &text,
                    options(no_check, no_emit),
                );
                // The public compile result retains the definitive syntax
                // diagnostic and also combines the checker's local nonclaim
                // for a recovered literal. `noCheck` omits that semantic phase.
                let semantic_complete = no_check || (case.terminated && !case.invalid);
                let expected_completion = if semantic_complete {
                    SemanticCompletion::Complete
                } else {
                    SemanticCompletion::Deferred
                };
                assert_eq!(
                    output.semantic_completion, expected_completion,
                    "row {}",
                    case.row
                );
                assert_eq!(
                    output.stats.semantic_completion, expected_completion,
                    "row {}",
                    case.row
                );
                assert_eq!(
                    diagnostics(&output.diagnostics),
                    expected_diagnostics(&case),
                    "row {}",
                    case.row
                );
                let expected_status = if !semantic_complete {
                    CompileExitStatus::SemanticIncomplete
                } else if case.diagnostics.is_empty() {
                    CompileExitStatus::Success
                } else if no_emit {
                    CompileExitStatus::DiagnosticsPresentOutputsSkipped
                } else {
                    CompileExitStatus::DiagnosticsPresentOutputsGenerated
                };
                assert_eq!(output.exit_status, expected_status, "row {}", case.row);
                assert_eq!(output.emitted_files.is_empty(), no_emit, "row {}", case.row);
            }
        }
    }
}

#[test]
fn unrepresentable_utf16_widens_only_for_mutable_values() {
    let mutable = compile(
        "mutable.ts",
        r#"var value = "\u{D800}";"#,
        options(false, false),
    );
    assert_eq!(mutable.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(mutable.exit_status, CompileExitStatus::Success);

    let exact_source = r#"const value: "\u{D800}" = "\u{D800}";"#;
    let exact = compile("exact.ts", exact_source, options(false, false));
    assert_eq!(exact.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(exact.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(exact.diagnostics.is_empty());

    let unchecked = compile("exact.ts", exact_source, options(true, false));
    assert_eq!(unchecked.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(unchecked.exit_status, CompileExitStatus::Success);
}

#[test]
fn es2015_emit_preserves_authored_utf16_comments_and_unterminated_recovery() {
    for case in cases() {
        let text = source_text(&case, "");
        let output = compile(
            &format!("row{:02}.ts", case.row),
            &text,
            options(false, false),
        );
        let expected = format!("\"use strict\";\n{}var x = {};\n", case.comments, case.raw);
        assert_eq!(javascript(&output), expected, "row {}", case.row);
    }
}

#[test]
fn row24_eof_and_physical_line_break_recovery_are_distinct() {
    let case = cases().into_iter().find(|case| case.row == 24).unwrap();
    for terminator in ["\n", "\r", "\r\n"] {
        let text = source_text(&case, terminator);
        let source = SourceText::new(
            FileId(8),
            PathBuf::from("row24.ts"),
            Arc::<str>::from(text.clone()),
        );
        let parsed = parse_source(&source);
        assert_eq!(
            diagnostics(&parsed.diagnostics),
            vec![(1199, 27, 0, UNICODE_EOF)],
            "{terminator:?}"
        );
        if terminator != "\r" {
            let output = compile("row24.ts", &text, options(false, false));
            assert_eq!(
                javascript(&output),
                concat!("\"use strict\";\n", r#"var x = "\u{00000000000067;"#, "\n"),
                "{terminator:?}"
            );
        }
    }

    let emit_staged = concat!("\r\n", r#"var x = "\u{00000000000067"#, "\r\n");
    let output = compile("row24.ts", emit_staged, options(false, false));
    assert_eq!(
        diagnostics(&output.diagnostics),
        vec![(1199, 28, 0, UNICODE_EOF)]
    );
}

#[test]
fn row25_single_valid_escape_missing_quote_is_owned_at_eof_and_line_break() {
    let case = cases().into_iter().find(|case| case.row == 25).unwrap();
    for terminator in ["\n", "\r\n"] {
        let text = source_text(&case, terminator);
        let output = compile("row25.ts", &text, options(false, false));
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert_eq!(
            diagnostics(&output.diagnostics),
            vec![(1002, 28, 0, STRING_EOF)],
            "{terminator:?}"
        );
        assert_eq!(
            javascript(&output),
            concat!("\"use strict\";\n", r#"var x = "\u{00000000000067};"#, "\n")
        );
    }

    let emit_staged = concat!("\r\n", r#"var x = "\u{00000000000067}"#, "\r\n");
    let output = compile("row25.ts", emit_staged, options(true, false));
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostics(&output.diagnostics),
        vec![(1002, 29, 0, STRING_EOF)]
    );
}

#[test]
fn plain_strings_templates_and_regular_expressions_remain_adjacent_syntax() {
    let source = SourceText::new(
        FileId(9),
        PathBuf::from("adjacent.ts"),
        Arc::<str>::from(r#"var plain = "text";"#),
    );
    let parsed = parse_source(&source);
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("expected one plain variable");
    };
    let StatementKind::Variable(declaration) = &statement.kind else {
        panic!("expected plain variable");
    };
    assert!(matches!(
        declaration.declarators[0]
            .initializer
            .as_ref()
            .map(|expression| &expression.kind),
        Some(ExpressionKind::Literal(Literal::String(StringLiteral::Plain(value)))) if value == "text"
    ));

    let escaped_source = SourceText::new(
        FileId(10),
        PathBuf::from("escaped-backslash.ts"),
        Arc::<str>::from(r#"var value = "\\u{61}";"#),
    );
    let escaped_scan = scan_source(&escaped_source);
    let escaped = parse_source(&escaped_source);
    assert!(escaped_scan.diagnostics.is_empty());
    assert!(escaped.diagnostics.is_empty());
    let [statement] = escaped.unit.statements.as_slice() else {
        panic!("escaped-backslash source did not parse as one statement");
    };
    let StatementKind::Variable(declaration) = &statement.kind else {
        panic!("escaped-backslash source did not parse as a variable");
    };
    assert!(matches!(
        declaration.declarators[0]
            .initializer
            .as_ref()
            .map(|expression| &expression.kind),
        Some(ExpressionKind::Literal(Literal::String(StringLiteral::Plain(value))))
            if value == r#"\u{61}"#
    ));

    for source in ["`plain`;", "/plain/g;"] {
        let output = compile("adjacent.ts", source, CompilerOptions::default());
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| !matches!(diagnostic.code, 1125 | 1126 | 1198 | 1199))
        );
    }
}
