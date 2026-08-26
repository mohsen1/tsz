use std::path::PathBuf;
use std::sync::Arc;

use crate::source::{FileId, SourceText};
use crate::syntax::{ExpressionKind, Literal, StatementKind, StringLiteral, parse_source};
use crate::{Compiler, CompilerOptions, SourceInput};

struct EscapeCase {
    raw: &'static str,
    code: u32,
    start: u32,
    length: u32,
    message: &'static str,
    value: &'static [u16],
}

const OCTAL_X: &str = "Octal escape sequences are not allowed. Use the syntax '\\x78'.";
const OCTAL_NUL: &str = "Octal escape sequences are not allowed. Use the syntax '\\x00'.";
const HEX: &str = "Hexadecimal digit expected.";

const CASES: &[EscapeCase] = &[
    EscapeCase {
        raw: r#""\170""#,
        code: 1487,
        start: 15,
        length: 4,
        message: OCTAL_X,
        value: &[b'x' as u16],
    },
    EscapeCase {
        raw: r#""\08""#,
        code: 1487,
        start: 15,
        length: 2,
        message: OCTAL_NUL,
        value: &[0, b'8' as u16],
    },
    EscapeCase {
        raw: r#""\8""#,
        code: 1488,
        start: 15,
        length: 2,
        message: "Escape sequence '\\8' is not allowed.",
        value: &[b'8' as u16],
    },
    EscapeCase {
        raw: r#""\9""#,
        code: 1488,
        start: 15,
        length: 2,
        message: "Escape sequence '\\9' is not allowed.",
        value: &[b'9' as u16],
    },
    EscapeCase {
        raw: r#""\x""#,
        code: 1125,
        start: 17,
        length: 0,
        message: HEX,
        value: &[b'\\' as u16, b'x' as u16],
    },
    EscapeCase {
        raw: r#""\x1""#,
        code: 1125,
        start: 18,
        length: 0,
        message: HEX,
        value: &[b'\\' as u16, b'x' as u16, b'1' as u16],
    },
    EscapeCase {
        raw: r#""\u""#,
        code: 1125,
        start: 17,
        length: 0,
        message: HEX,
        value: &[b'\\' as u16, b'u' as u16],
    },
    EscapeCase {
        raw: r#""\u123""#,
        code: 1125,
        start: 20,
        length: 0,
        message: HEX,
        value: &[
            b'\\' as u16,
            b'u' as u16,
            b'1' as u16,
            b'2' as u16,
            b'3' as u16,
        ],
    },
];

fn options(no_check: bool) -> CompilerOptions {
    CompilerOptions {
        no_check,
        no_emit: true,
        target: "es2025".to_string(),
        module: "preserve".to_string(),
        ..CompilerOptions::default()
    }
}

#[test]
fn invalid_ordinary_escape_diagnostics_match_ts7_with_and_without_checking() {
    for case in CASES {
        let source = format!("const value = {};", case.raw);
        for no_check in [false, true] {
            let output = Compiler::new().compile(
                vec![SourceInput::new(
                    "case.ts",
                    Arc::<str>::from(source.clone()),
                )],
                &options(no_check),
            );
            let [diagnostic] = output.diagnostics.as_slice() else {
                panic!(
                    "expected one diagnostic for {} (no_check={no_check}): {:#?}",
                    case.raw, output.diagnostics
                );
            };
            assert_eq!(
                (
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.message_text.as_str(),
                ),
                (case.code, case.start, case.length, case.message),
                "{} (no_check={no_check})",
                case.raw
            );
        }
    }
}

#[test]
fn invalid_and_valid_ordinary_escapes_keep_ts7_cooked_identity() {
    for (raw, expected) in CASES.iter().map(|case| (case.raw, case.value)).chain([
        (r#""\0""#, &[0][..]),
        (r#""\x78""#, &[b'x' as u16][..]),
        (r#""\u0078""#, &[b'x' as u16][..]),
    ]) {
        let source = SourceText::new(FileId(0), PathBuf::from("case.ts"), Arc::<str>::from(raw));
        let parsed = parse_source(&source);
        let [statement] = parsed.unit.statements.as_slice() else {
            panic!("expected one expression for {raw:?}");
        };
        let StatementKind::Expression(expression) = &statement.kind else {
            panic!("expected expression statement for {raw:?}");
        };
        let ExpressionKind::Literal(Literal::String(StringLiteral::Plain(value))) =
            &expression.kind
        else {
            panic!("expected ordinary string literal for {raw:?}");
        };
        assert_eq!(
            value.encode_utf16().collect::<Vec<_>>(),
            expected,
            "{raw:?}"
        );
    }
}

#[test]
fn valid_controls_are_clean_and_authored_spelling_survives_javascript_emit() {
    let source = concat!(
        "const nul = \"\\0\";\n",
        "const hex = \"\\x78\";\n",
        "const unicode = \"\\u0078\";\n",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            target: "es2025".to_string(),
            module: "preserve".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("JavaScript output");
    assert!(javascript.text.contains(r#""\0""#));
    assert!(javascript.text.contains(r#""\x78""#));
    assert!(javascript.text.contains(r#""\u0078""#));

    let invalid_source = CASES
        .iter()
        .enumerate()
        .map(|(index, case)| format!("const value{index} = {};\n", case.raw))
        .collect::<String>();
    let invalid_output = Compiler::new().compile(
        vec![SourceInput::new(
            "invalid.ts",
            Arc::<str>::from(invalid_source),
        )],
        &CompilerOptions {
            no_check: true,
            target: "es2025".to_string(),
            module: "preserve".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(invalid_output.diagnostics.len(), CASES.len());
    let invalid_javascript = invalid_output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("JavaScript output with syntax diagnostics");
    for case in CASES {
        assert!(
            invalid_javascript.text.contains(case.raw),
            "authored spelling {} was not preserved in {}",
            case.raw,
            invalid_javascript.text
        );
    }
}
