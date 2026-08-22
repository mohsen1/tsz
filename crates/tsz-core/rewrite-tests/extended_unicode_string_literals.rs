use std::path::PathBuf;
use std::sync::Arc;

use tsz::diagnostics::Diagnostic;
use tsz::service::LanguageService;
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
        let Some(initializer) = &declaration.initializer else {
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
        assert!(
            literal.validation_supported(),
            "row {} validation",
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
                assert_eq!(
                    output.semantic_completion,
                    SemanticCompletion::Complete,
                    "row {}",
                    case.row
                );
                assert_eq!(
                    output.stats.semantic_completion,
                    SemanticCompletion::Complete,
                    "row {}",
                    case.row
                );
                assert_eq!(
                    diagnostics(&output.diagnostics),
                    expected_diagnostics(&case),
                    "row {}",
                    case.row
                );
                let expected_status = if case.diagnostics.is_empty() {
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
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
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

fn assert_incomplete(path: &str, source: &str, options: CompilerOptions) {
    let output = compile(path, source, options);
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Deferred,
        "{path}: {source:?}"
    );
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Deferred,
        "{path}: {source:?}"
    );
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(output.emitted_files.is_empty());
}

#[test]
fn exact_option_gate_accepts_only_the_owned_product_matrix() {
    let source = r#"var owned = "\u{67}";"#;
    for target in ["es6", "ES6", "es2015", "ES2015"] {
        for module in ["commonjs", "esnext", "preserve"] {
            for no_check in [false, true] {
                for no_emit in [false, true] {
                    let mut supported = options(no_check, no_emit);
                    supported.target = target.to_string();
                    supported.module = module.to_string();
                    let output = compile("owned.ts", source, supported);
                    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
                    assert_eq!(output.exit_status, CompileExitStatus::Success);
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
        assert_incomplete("owned.ts", source, candidate);
    }
}

#[test]
fn safe_host_boundary_rejects_adjacent_unowned_shapes_and_source_kinds() {
    let unsupported = [
        r#"let value = "\u{67}";"#,
        r#"const value = "\u{67}";"#,
        r#"var value: string = "\u{67}";"#,
        r#"export var value = "\u{67}";"#,
        r#"var value = ("\u{67}");"#,
        r#""\u{67}";"#,
        r#"var value = { key: "\u{67}" };"#,
        r#"var value = ["\u{67}"];"#,
        r#"type Value = "\u{67}";"#,
        r#"import value from "\u{67}";"#,
        r#""\u{67}"; var value = 1;"#,
        r#"function f() { var value = "\u{67}"; }"#,
        r#"var first = "\u{67}"; var second = "\u{68}";"#,
        r#"var value = "plain\n\u{67}";"#,
        r#"var value = "prefix\u{67}";"#,
        r#"var value = '\u{67}';"#,
    ];
    for source in unsupported {
        for no_check in [false, true] {
            assert_incomplete("unsupported.ts", source, options(no_check, false));
        }
    }
    for path in [
        "value.js",
        "value.jsx",
        "value.tsx",
        "value.mts",
        "value.cts",
        "value.d.ts",
    ] {
        assert_incomplete(path, r#"var value = "\u{67}";"#, options(false, false));
    }
}

#[test]
fn canonical_unicode_comments_are_owned_but_other_comment_geometry_defers() {
    for line_break in ["\n", "\r\n"] {
        let source = format!(
            "//  2. Let cu1 be floor((cp – 65536) / 1024).{line_break}// Unicode ≤ maximum.{line_break}var value = \"\\u{{D800}}\";"
        );
        let output = compile("comments.ts", &source, options(false, false));
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert!(javascript(&output).contains("// Unicode ≤ maximum.\n"));
    }

    let unsupported = [
        "// plain\n\nvar value = \"\\u{67}\";",
        " // indented\nvar value = \"\\u{67}\";",
        "var value = \"\\u{67}\"; // trailing",
        "/* block */\nvar value = \"\\u{67}\";",
        "/** doc */\nvar value = \"\\u{67}\";",
        "/// triple\nvar value = \"\\u{67}\";",
        "// @directive\nvar value = \"\\u{67}\";",
        "// plain\u{2028}var value = \"\\u{67}\";",
        "var valué = \"\\u{67}\";",
    ];
    for source in unsupported {
        assert_incomplete("comments.ts", source, options(false, false));
    }
    assert_incomplete(
        "comments.ts",
        "#!/usr/bin/env node\nvar value = \"\\u{67}\";",
        options(false, false),
    );
}

#[test]
fn safe_roots_are_reversed_renamed_and_binder_unique() {
    let compiler = Compiler::new();
    let roots = || {
        vec![
            SourceInput::new("a.ts", Arc::<str>::from(r#"var alpha = "\u{D800}";"#)),
            SourceInput::new("b.ts", Arc::<str>::from(r#"var beta = "\u{10FFFF}";"#)),
        ]
    };
    let expected = compiler.compile(roots(), &options(false, false));
    assert_eq!(expected.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(expected.emitted_files.len(), 2);
    for iteration in 0..10 {
        let mut inputs = roots();
        if iteration % 2 == 1 {
            inputs.reverse();
        }
        let actual = compiler.compile(inputs, &options(false, false));
        assert_eq!(actual.semantic_completion, expected.semantic_completion);
        assert_eq!(actual.diagnostics, expected.diagnostics);
        assert_eq!(actual.emitted_files.len(), expected.emitted_files.len());
        for (actual, expected) in actual.emitted_files.iter().zip(&expected.emitted_files) {
            assert_eq!(actual.path, expected.path);
            assert_eq!(actual.text, expected.text);
        }
        assert_eq!(actual.stats.types, expected.stats.types);
    }

    let duplicate = Compiler::new().compile(
        vec![
            SourceInput::new("a.ts", Arc::<str>::from(r#"var duplicate = "\u{67}";"#)),
            SourceInput::new("b.ts", Arc::<str>::from(r#"var duplicate = "\u{68}";"#)),
        ],
        &options(false, false),
    );
    assert_eq!(duplicate.semantic_completion, SemanticCompletion::Deferred);
    assert!(duplicate.emitted_files.is_empty());

    let mixed = Compiler::new().compile(
        vec![
            SourceInput::new("a.ts", Arc::<str>::from(r#"var alpha = "\u{67}";"#)),
            SourceInput::new("b.ts", Arc::<str>::from("var beta = \"plain\";")),
        ],
        &options(false, false),
    );
    assert_eq!(mixed.semantic_completion, SemanticCompletion::Deferred);
    assert!(mixed.emitted_files.is_empty());
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
        declaration.initializer.as_ref().map(|expression| &expression.kind),
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
    assert!(!escaped.unit.has_authored_extended_unicode_string());
    let [statement] = escaped.unit.statements.as_slice() else {
        panic!("escaped-backslash source did not parse as one statement");
    };
    let StatementKind::Variable(declaration) = &statement.kind else {
        panic!("escaped-backslash source did not parse as a variable");
    };
    assert!(matches!(
        declaration.initializer.as_ref().map(|expression| &expression.kind),
        Some(ExpressionKind::Literal(Literal::String(StringLiteral::Plain(value))))
            if value == r#"\\u{61}"#
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

#[test]
fn unproved_recovery_combinations_are_deferred_in_every_product_mode() {
    let unsupported = [
        r#"var value = "\u{110000x";"#,
        r#"var value = "\u{110000"#,
        r#"var value = "\u{@}";"#,
        r#"var value = "\u{_}";"#,
        r#"var value = "\u{$}";"#,
        r#"var value = "\u{gh}";"#,
        r#"var value = "\u{67}tail";"#,
        r#"var value = "\u{r}tail";"#,
        r#"var value = "\u{67}\u{r}";"#,
        r#"var value = "\u{r}\u{67}";"#,
        r#"var value = "\u{r}\u{-DDDD}";"#,
        r#"var value = "\u{110000}"#,
        r#"var value = "\u{}"#,
        r#"var value = "\u{67}\u{68}"#,
        r#"var value = "\u{110000}\u{120000}";"#,
        r#"var value = "\u{}\u{}";"#,
    ];
    for source in unsupported {
        for no_check in [false, true] {
            for no_emit in [false, true] {
                assert_incomplete("recovery.ts", source, options(no_check, no_emit));
            }
        }
    }
}

#[test]
fn attribution_bindings_and_line_geometry_fail_closed() {
    for source in [
        r#"var undefined = "\u{67}";"#,
        r#"var Symbol = "\u{67}";"#,
        concat!(r#"var value = "\u{67}";"#, "\n/*"),
        concat!("\u{feff}", r#"var value = "\u{67}";"#),
        concat!("// plain\u{2028}", r#"var value = "\u{67}";"#),
        concat!("// plain\u{2029}", r#"var value = "\u{67}";"#),
        concat!(r#"var value = "\u{67}";"#, "\r"),
    ] {
        for no_check in [false, true] {
            for no_emit in [false, true] {
                assert_incomplete("attribution.ts", source, options(no_check, no_emit));
            }
        }
    }

    let six_comments = concat!(
        "// first\n",
        "// second\n",
        "// third ≤\n",
        "// fourth –\n",
        "// fifth\n",
        "// sixth\n",
        r#"var value = "\u{D800}";"#,
    );
    let output = compile("comments.ts", six_comments, options(false, false));
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        javascript(&output),
        concat!(
            "\"use strict\";\n",
            "// first\n",
            "// second\n",
            "// third ≤\n",
            "// fourth –\n",
            "// fifth\n",
            "// sixth\n",
            r#"var value = "\u{D800}";"#,
            "\n",
        )
    );
}

#[test]
fn service_publishes_only_the_program_owned_direct_var_widening() {
    let mut service = LanguageService::new(options(false, true));
    service.open("service.ts", r#"var value = "\u{D800}";"#);
    let info = service
        .quick_info("service.ts", 6)
        .expect("owned direct var has string quick info");
    assert_eq!(info.display, "var value: string");

    for source in [
        r#"const value = "\u{D800}";"#,
        r#"let value = "\u{67}";"#,
        r#"type Value = "\u{67}";"#,
        r#"const value = { nested: "\u{67}" };"#,
        r#"var value = "\u{@}";"#,
    ] {
        assert!(service.change("service.ts", source));
        assert!(service.quick_info("service.ts", 6).is_none(), "{source:?}");
    }

    assert!(service.change("service.ts", r#"var value = "\u{D800}";"#));
    service.open("mixed.ts", "var other = \"plain\";");
    assert!(service.quick_info("service.ts", 6).is_none());
}
