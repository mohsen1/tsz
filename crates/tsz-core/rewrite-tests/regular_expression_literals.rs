use std::path::PathBuf;
use std::sync::Arc;

use tsz::source::{FileId, SourceText};
use tsz::syntax::{ExpressionKind, StatementKind, TokenKind, parse_source, scan_source};
use tsz::{
    CompileExitStatus, CompileOutput, Compiler, CompilerOptions, SemanticCompletion, SourceInput,
};

fn options(no_check: bool) -> CompilerOptions {
    CompilerOptions {
        no_check,
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
        .unwrap_or_else(|| {
            panic!(
                "expected JavaScript product: {:?} {:?}",
                output.semantic_completion, output.diagnostics
            )
        })
        .text
        .as_str()
}

fn codes(output: &CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn expected_script(source: &str) -> String {
    format!("\"use strict\";\n{source}\n")
}

const CLEAN_JS_MANIFEST: &[(&str, &str, &str)] = &[
    (
        "unicodeExtendedEscapesInRegularExpressions01",
        r"var x = /\u{0}/gu;",
        concat!("\"use strict\";\n", r"var x = /\u{0}/gu;", "\n"),
    ),
    (
        "unicodeExtendedEscapesInRegularExpressions02",
        r"var x = /\u{00}/gu;",
        concat!("\"use strict\";\n", r"var x = /\u{00}/gu;", "\n"),
    ),
    (
        "unicodeExtendedEscapesInRegularExpressions03",
        r"var x = /\u{0000}/gu;",
        concat!("\"use strict\";\n", r"var x = /\u{0000}/gu;", "\n"),
    ),
    (
        "unicodeExtendedEscapesInRegularExpressions04",
        r"var x = /\u{00000000}/gu;",
        concat!("\"use strict\";\n", r"var x = /\u{00000000}/gu;", "\n",),
    ),
    (
        "unicodeExtendedEscapesInRegularExpressions05",
        r"var x = /\u{48}\u{65}\u{6c}\u{6c}\u{6f}\u{20}\u{77}\u{6f}\u{72}\u{6c}\u{64}/gu;",
        concat!(
            "\"use strict\";\n",
            r"var x = /\u{48}\u{65}\u{6c}\u{6c}\u{6f}\u{20}\u{77}\u{6f}\u{72}\u{6c}\u{64}/gu;",
            "\n",
        ),
    ),
    (
        "unicodeExtendedEscapesInRegularExpressions13",
        r"var x = /\u{DDDDD}/gu;",
        concat!("\"use strict\";\n", r"var x = /\u{DDDDD}/gu;", "\n"),
    ),
    (
        "unicodeExtendedEscapesInRegularExpressions15",
        r"var x = /\u{abcd}\u{ef12}\u{3456}\u{7890}/gu;",
        concat!(
            "\"use strict\";\n",
            r"var x = /\u{abcd}\u{ef12}\u{3456}\u{7890}/gu;",
            "\n",
        ),
    ),
    (
        "unicodeExtendedEscapesInRegularExpressions16",
        r"var x = /\u{ABCD}\u{EF12}\u{3456}\u{7890}/gu;",
        concat!(
            "\"use strict\";\n",
            r"var x = /\u{ABCD}\u{EF12}\u{3456}\u{7890}/gu;",
            "\n",
        ),
    ),
    (
        "unicodeExtendedEscapesInRegularExpressions18",
        r"var x = /\u{65}\u{65}/gu;",
        concat!("\"use strict\";\n", r"var x = /\u{65}\u{65}/gu;", "\n",),
    ),
    (
        "parserRegularExpression1",
        r"/(#?-?\d*\.\d\w*%?)|(@?#?[\w-?]+%?)/g;",
        concat!(
            "\"use strict\";\n",
            r"/(#?-?\d*\.\d\w*%?)|(@?#?[\w-?]+%?)/g;",
            "\n",
        ),
    ),
    (
        "parser596700",
        r"var regex2 = /[a-z/]$/i;",
        concat!("\"use strict\";\n", r"var regex2 = /[a-z/]$/i;", "\n",),
    ),
    (
        "parser645086_3",
        r"var v = /[\]/]/",
        concat!("\"use strict\";\n", r"var v = /[\]/]/;", "\n"),
    ),
    (
        "parser645086_4",
        r"var v = /[^\]/]/",
        concat!("\"use strict\";\n", r"var v = /[^\]/]/;", "\n"),
    ),
];

#[test]
fn regex_has_a_dedicated_expression_node_and_preserves_scanner_metadata() {
    let text = r"var renamed = /a\/[b-d]+/giu;";
    let source = SourceText::new(
        FileId(9),
        PathBuf::from("syntax.ts"),
        Arc::<str>::from(text),
    );
    let parsed = parse_source(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("expected one statement");
    };
    let StatementKind::Variable(declaration) = &statement.kind else {
        panic!("expected variable declaration");
    };
    let Some(initializer) = &declaration.initializer else {
        panic!("expected initializer");
    };
    assert!(
        matches!(initializer.kind, ExpressionKind::RegularExpression(_)),
        "regex must not enter literal freshness machinery: {:?}",
        initializer.kind
    );
    assert_eq!(source.slice(initializer.span), r"/a\/[b-d]+/giu");
}

#[test]
fn exact_thirteen_row_javascript_manifest_is_frozen_in_both_check_modes() {
    assert_eq!(CLEAN_JS_MANIFEST.len(), 13);
    assert_eq!(
        CLEAN_JS_MANIFEST
            .iter()
            .map(|(_, _, expected_javascript)| expected_javascript.len())
            .sum::<usize>(),
        589,
        "aggregate UTF-8 bytes in the exact expected JavaScript outputs",
    );
    for no_check in [false, true] {
        for (name, source, expected_javascript) in CLEAN_JS_MANIFEST {
            let output = compile(&format!("{name}.ts"), source, options(no_check));
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "{name} noCheck={no_check}: {:?}",
                output.diagnostics
            );
            assert_eq!(output.exit_status, CompileExitStatus::Success, "{name}");
            assert!(
                output.diagnostics.is_empty(),
                "{name}: {:?}",
                output.diagnostics
            );
            assert_eq!(javascript(&output), *expected_javascript, "{name}");
        }
    }
}

#[test]
fn ascii_unicode_escape_invalid_siblings_match_checked_diagnostics_and_no_check_emit() {
    let cases: &[(&str, &str, &[u32])] = &[
        ("12", r"var x = /\u{FFFFFFFF}/gu;", &[1198]),
        (
            "14",
            concat!(
                "// Shouldn't work, negatives are not allowed.\n",
                r"var x = /\u{-DDDD}/gu;",
            ),
            &[1125, 1508],
        ),
        (
            "17",
            r"var x = /\u{r}\u{n}\u{t}/gu;",
            &[1125, 1508, 1125, 1508, 1125, 1508],
        ),
        ("19", r"var x = /\u{}/gu;", &[1125]),
    ];

    for (name, source, expected_codes) in cases {
        let checked = compile(&format!("invalid-{name}.ts"), source, options(false));
        assert_eq!(
            checked.semantic_completion,
            SemanticCompletion::Complete,
            "{name}: {:?}",
            checked.diagnostics
        );
        assert_eq!(
            checked.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        );
        assert_eq!(codes(&checked), *expected_codes, "{name}");
        let expected_facts = match *name {
            "12" => vec![(
                1198,
                source.find("FFFFFFFF").unwrap() as u32,
                8,
                "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.",
            )],
            "14" => {
                let start = source.find("-DDDD").unwrap() as u32;
                vec![
                    (1125, start, 1, "Hexadecimal digit expected."),
                    (
                        1508,
                        start + 5,
                        1,
                        "Unexpected '}'. Did you mean to escape it with backslash?",
                    ),
                ]
            }
            "17" => ["r", "n", "t"]
                .into_iter()
                .flat_map(|payload| {
                    let escape = format!(r"\u{{{payload}}}");
                    let start = source.find(&escape).unwrap() as u32 + 3;
                    [
                        (1125, start, 1, "Hexadecimal digit expected."),
                        (
                            1508,
                            start + 1,
                            1,
                            "Unexpected '}'. Did you mean to escape it with backslash?",
                        ),
                    ]
                })
                .collect(),
            "19" => vec![(
                1125,
                source.find(r"\u{}").unwrap() as u32 + 3,
                1,
                "Hexadecimal digit expected.",
            )],
            _ => unreachable!(),
        };
        let actual_facts = checked
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.message_text.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual_facts, expected_facts, "{name}");
        assert_eq!(javascript(&checked), expected_script(source), "{name}");

        let unchecked = compile(&format!("invalid-{name}.ts"), source, options(true));
        assert_eq!(unchecked.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(unchecked.exit_status, CompileExitStatus::Success);
        assert!(
            unchecked.diagnostics.is_empty(),
            "{name}: {:?}",
            unchecked.diagnostics
        );
        assert_eq!(javascript(&unchecked), javascript(&checked), "{name}");
    }

    let out_of_range = compile("span.ts", r"var x = /\u{110000}/gu;", options(false));
    let [diagnostic] = out_of_range.diagnostics.as_slice() else {
        panic!("expected one diagnostic");
    };
    assert_eq!(
        (
            diagnostic.code,
            diagnostic.start,
            diagnostic.length,
            diagnostic.message_text.as_str(),
        ),
        (
            1198,
            12,
            6,
            "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.",
        )
    );
}

#[test]
fn checked_flag_validation_is_suppressed_by_no_check() {
    let cases = [
        (r"/x/z;", 1499, 3, "Unknown regular expression flag."),
        (r"/x/gg;", 1500, 4, "Duplicate regular expression flag."),
    ];
    for (source, code, start, message) in cases {
        let checked = compile("flag.ts", source, options(false));
        let [diagnostic] = checked.diagnostics.as_slice() else {
            panic!("expected one diagnostic for {source:?}");
        };
        assert_eq!(
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ),
            (code, start, 1, message)
        );
        assert_eq!(
            checked.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        );
        assert_eq!(javascript(&checked), expected_script(source));

        let unchecked = compile("flag.ts", source, options(true));
        assert!(unchecked.diagnostics.is_empty());
        assert_eq!(unchecked.exit_status, CompileExitStatus::Success);
        assert_eq!(javascript(&unchecked), javascript(&checked));
    }
}

#[test]
fn unterminated_direct_atom_has_syntax_diagnostic_in_both_modes_and_raw_emit() {
    for no_check in [false, true] {
        let output = compile("unterminated.ts", "/abc", options(no_check));
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("expected TS1161 under noCheck={no_check}");
        };
        assert_eq!(
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ),
            (1161, 0, 4, "Unterminated regular expression literal.")
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        );
        assert_eq!(javascript(&output), "\"use strict\";\n/abc;\n");
    }
}

#[test]
fn renamed_vars_bare_atoms_and_comment_removal_are_exact() {
    for no_check in [false, true] {
        for source in [
            r"var renamed9 = /first/g;",
            r"var _payload = /\u{41}/u;",
            r"var $payload = /third/i;",
            r"/first/g;",
        ] {
            let output = compile("hosts.ts", source, options(no_check));
            assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
            assert!(output.diagnostics.is_empty());
            assert_eq!(javascript(&output), expected_script(source));
        }

        let source = concat!(
            "// first\n",
            "// second\n",
            "// third\n",
            "// fourth\n",
            "// fifth\n",
            "// sixth\n",
            "var renamed = /x/g;",
        );
        let preserved = compile("comments.ts", source, options(no_check));
        assert_eq!(preserved.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(javascript(&preserved), expected_script(source));

        let removed = compile(
            "comments.ts",
            source,
            CompilerOptions {
                remove_comments: true,
                ..options(no_check)
            },
        );
        assert_eq!(removed.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            javascript(&removed),
            "\"use strict\";\nvar renamed = /x/g;\n"
        );
    }
}

#[test]
fn division_is_unchanged_and_broader_regex_hosts_fail_closed() {
    for no_check in [false, true] {
        let division = compile(
            "division.ts",
            "var quotient = 8 / 2 / 2;",
            options(no_check),
        );
        assert_eq!(division.semantic_completion, SemanticCompletion::Complete);
        assert!(division.diagnostics.is_empty());
        assert_eq!(
            javascript(&division),
            "\"use strict\";\nvar quotient = 8 / 2 / 2;\n"
        );

        for source in [
            "/x/.source;",
            "/x/(\"x\");",
            "/x/ `body`;",
            "if (true) /x/;",
            "function read() { return /x/; }",
            "var wrapped = (/x/);",
            "let mutable = /x/;",
            "const fixed = /x/;",
            "var typed: RegExp = /x/;",
            "export var exposed = /x/;",
            "var first = /x/; var sibling = 1;",
            "var first = /x/; /y/;",
            "/x/; /y/;",
            "var first = /x/; var second = /y/;",
            "var first = /x/, second = /y/;",
            "// comment before bare\n/x/;",
            concat!(
                "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
                "//  1. Assert: 0 ≤ cp ≤ 0x10FFFF.\n",
                r"var x = /\u{10FFFF}/gu;",
            ),
            concat!(
                "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
                "//  1. Assert: 0 ≤ cp ≤ 0x10FFFF.\n",
                r"var x = /\u{110000}/gu;",
            ),
            concat!(
                "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
                "//  2. If cp ≤ 65535, return cp.\n",
                "// (FFFF == 65535)\n",
                r"var x = /\u{FFFF}/gu;",
            ),
            concat!(
                "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
                "//  2. If cp ≤ 65535, return cp.\n",
                "// (10000 == 65536)\n",
                r"var x = /\u{10000}/gu;",
            ),
            concat!(
                "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
                "//  2. Let cu1 be floor((cp – 65536) / 1024) + 0xD800.\n",
                "// Although we should just get back a single code point value of 0xD800,\n",
                "// this is a useful edge-case test.\n",
                r"var x = /\u{D800}/gu;",
            ),
            concat!(
                "// ES6 Spec - 10.1.1 Static Semantics: UTF16Encoding (cp)\n",
                "//  2. Let cu2 be ((cp – 65536) modulo 1024) + 0xDC00.\n",
                "// Although we should just get back a single code point value of 0xDC00,\n",
                "// this is a useful edge-case test.\n",
                r"var x = /\u{DC00}/gu;",
            ),
            "// café\nvar value = /x/z;",
            "// 😀\nvar value = /x/z;",
            "var first = /x/; // trailing",
            "// trailing whitespace \nvar first = /x/;",
            "/// triple slash\nvar first = /x/;",
            "// plain\n/* block */\nvar first = /x/;",
            "/* block */\nvar first = /x/;",
            "// @ts-ignore\nvar first = /x/;",
            "// detached\n\nvar first = /x/;",
            " // indented\nvar first = /x/;",
            "// plain\rvar first = /\\u{}/gu;\r",
            "\rvar first = /\\u{}/gu;",
            "\u{00a0}var first = /\\u{}/gu;",
            "\u{2003}var first = /\\u{}/gu;",
            " var first = /x/;",
            "\u{feff}var first = /\\u{}/gu;",
            "var first = \u{00a0}/\\u{}/gu;",
            "\u{2028}\nvar first = /\\u{}/gu;",
            "\u{2029}\nvar first = /\\u{}/gu;",
            "// unicode separator\u{2028}var first = /x/;",
            "// unicode paragraph\u{2029}var first = /x/;",
            "/x/[\"source\"];",
            "target = /x/;",
            "var target: any; target = /x/;",
            "value > /x/;",
            "value >> /x/;",
            "value >>> /x/;",
            "value as /x/;",
            "await /x/;",
            r"async function f() { return await /[a-z\/]+/; }",
            r"function* g() { yield /[a-z\/]+/; }",
            r"void /[a-z\/]+/;",
            r"value > /[a-z\/]+/.source;",
            "while (true) /x/;",
            "with (object) /x/;",
            "for (const item of /x/) {}",
            "/x/s;",
            "/x/d;",
            "/x/uv;",
            "/[a&&b]/v;",
            r"/\u{65}/g;",
            r"/[z-a]/g;",
            r"/\b*/g;",
            r"/\-/u;",
            r"/[\B]/u;",
            r"/[a-\d]/u;",
            r"/[\d-a]/u;",
            r"/[\w-?]/u;",
            r"/[\u{5A}-\u{41}]/u;",
            r"/\u{1x2}/u;",
            r"/\u{r2}/u;",
            r"/\u{\}/u;",
            r"/\u{\x}/u;",
            r"/\u{(}/u;",
            "/(/g;",
            "/[/",
            r"/abc\",
            "/abc;",
            "/abc ",
            "var x = /abc;",
            "var x = /abc ",
            "/abc\n",
            "/abc\r\n",
            "/abc\\\n/;",
        ] {
            let output = compile("boundary.ts", source, options(no_check));
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "{source:?} noCheck={no_check}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "{source:?}");
        }
    }
}

#[test]
fn slash_equals_and_division_tokens_are_never_reclassified_as_regex_atoms() {
    let source = SourceText::new(
        FileId(11),
        PathBuf::from("slashes.ts"),
        Arc::<str>::from("value /= 2; quotient = 8 / 2 / 2;"),
    );
    let scanned = scan_source(&source);
    let kinds = scanned
        .tokens
        .iter()
        .map(|token| token.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == TokenKind::SlashEquals)
            .count(),
        1
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == TokenKind::Slash)
            .count(),
        2
    );
    assert!(!kinds.contains(&TokenKind::RegularExpressionLiteral));
}

#[test]
fn closing_generic_angles_keep_following_slashes_on_the_division_path() {
    let source = concat!(
        "type Identity<T> = T;\n",
        "var single = 4 as Identity<number> / 2;\n",
        "var double = 8 as Identity<Identity<number>> / 2;\n",
        "var triple = 16 as Identity<Identity<Identity<number>>> / 2;",
    );
    let scanned = scan_source(&SourceText::new(
        FileId(12),
        PathBuf::from("closing-angles.ts"),
        Arc::<str>::from(source),
    ));
    assert!(scanned.diagnostics.is_empty(), "{:?}", scanned.diagnostics);
    let slash_predecessors = scanned
        .tokens
        .windows(2)
        .filter_map(|tokens| (tokens[1].kind == TokenKind::Slash).then_some(tokens[0].kind))
        .collect::<Vec<_>>();
    assert_eq!(
        slash_predecessors,
        vec![
            TokenKind::GreaterThan,
            TokenKind::GreaterThanGreaterThan,
            TokenKind::GreaterThanGreaterThanGreaterThan,
        ],
    );
    assert!(
        !scanned
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::RegularExpressionLiteral)
    );

    for no_check in [false, true] {
        let output = compile("closing-angles.ts", source, options(no_check));
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "noCheck={no_check}: {:?}",
            output.diagnostics,
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(
            javascript(&output),
            concat!(
                "\"use strict\";\n",
                "var single = 4 / 2;\n",
                "var double = 8 / 2;\n",
                "var triple = 16 / 2;\n",
            ),
        );
    }

    let wrapped_type_source = concat!(
        "type Id<T> = T;\n",
        "var keyofValue = 32 as keyof Id<number> / 2;\n",
        "var unionVoid = 64 as number | void / 2;\n",
        "var unionReference = 128 as number | Id<number> / 2;",
    );
    let scanned_wrapped_types = scan_source(&SourceText::new(
        FileId(15),
        PathBuf::from("wrapped-as-types.ts"),
        Arc::<str>::from(wrapped_type_source),
    ));
    assert!(scanned_wrapped_types.diagnostics.is_empty());
    assert_eq!(
        scanned_wrapped_types
            .tokens
            .windows(2)
            .filter_map(|tokens| { (tokens[1].kind == TokenKind::Slash).then_some(tokens[0].kind) })
            .collect::<Vec<_>>(),
        vec![
            TokenKind::GreaterThan,
            TokenKind::Void,
            TokenKind::GreaterThan
        ],
    );
    assert!(
        !scanned_wrapped_types
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::RegularExpressionLiteral)
    );

    let unchecked_wrapped_types =
        compile("wrapped-as-types.ts", wrapped_type_source, options(true));
    assert_eq!(
        unchecked_wrapped_types.semantic_completion,
        SemanticCompletion::Complete,
        "{:?}",
        unchecked_wrapped_types.diagnostics,
    );
    assert!(unchecked_wrapped_types.diagnostics.is_empty());
    assert_eq!(
        javascript(&unchecked_wrapped_types),
        concat!(
            "\"use strict\";\n",
            "var keyofValue = 32 / 2;\n",
            "var unionVoid = 64 / 2;\n",
            "var unionReference = 128 / 2;\n",
        ),
    );

    let new_source = concat!(
        "type Id<T> = T;\n",
        "var C: any;\n",
        "var constructed = new C<Id<number>> / 2;",
    );
    let scanned_new = scan_source(&SourceText::new(
        FileId(15),
        PathBuf::from("new-type-arguments.ts"),
        Arc::<str>::from(new_source),
    ));
    assert!(
        scanned_new.diagnostics.is_empty(),
        "{:?}",
        scanned_new.diagnostics,
    );
    assert_eq!(
        scanned_new
            .tokens
            .windows(2)
            .filter_map(|tokens| { (tokens[1].kind == TokenKind::Slash).then_some(tokens[0].kind) })
            .collect::<Vec<_>>(),
        vec![TokenKind::GreaterThanGreaterThan],
    );
    assert!(
        !scanned_new
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::RegularExpressionLiteral)
    );

    for no_check in [false, true] {
        for no_emit in [false, true] {
            let new_output = compile(
                "new-type-arguments.ts",
                new_source,
                CompilerOptions {
                    no_emit,
                    ..options(no_check)
                },
            );
            assert_eq!(
                new_output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} noEmit={no_emit}: {:?}",
                new_output.diagnostics,
            );
            assert!(new_output.diagnostics.is_empty());
            assert!(new_output.emitted_files.is_empty());
        }
    }

    let parenthesized_generic_new = "class C<T> {} var constructed = new C<number>();";
    for no_check in [false, true] {
        for no_emit in [false, true] {
            let output = compile(
                "parenthesized-generic-new.ts",
                parenthesized_generic_new,
                CompilerOptions {
                    no_emit,
                    ..options(no_check)
                },
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "noCheck={no_check} noEmit={no_emit}: {:?}",
                output.diagnostics,
            );
            assert!(output.diagnostics.is_empty());
            assert!(output.emitted_files.is_empty());
        }
    }

    for no_check in [false, true] {
        let plain_new = compile(
            "plain-new.ts",
            "var C: any; var constructed = new C();",
            options(no_check),
        );
        assert_eq!(
            plain_new.semantic_completion,
            SemanticCompletion::Complete,
            "noCheck={no_check}: {:?}",
            plain_new.diagnostics,
        );
        assert!(plain_new.diagnostics.is_empty());
        assert_eq!(
            javascript(&plain_new),
            "\"use strict\";\nvar C;\nvar constructed = new C();\n",
        );
    }

    let call_source = concat!(
        "type Id<T> = T;\n",
        "var factory: any;\n",
        "var called = factory<Id<number>>() / 2;",
    );
    let scanned_call = scan_source(&SourceText::new(
        FileId(16),
        PathBuf::from("call-type-arguments.ts"),
        Arc::<str>::from(call_source),
    ));
    assert!(scanned_call.diagnostics.is_empty());
    assert_eq!(
        scanned_call
            .tokens
            .windows(2)
            .filter_map(|tokens| { (tokens[1].kind == TokenKind::Slash).then_some(tokens[0].kind) })
            .collect::<Vec<_>>(),
        vec![TokenKind::RightParen],
    );
    assert!(
        !scanned_call
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::RegularExpressionLiteral)
    );

    let unchecked_call = compile("call-type-arguments.ts", call_source, options(true));
    assert_eq!(
        unchecked_call.semantic_completion,
        SemanticCompletion::Complete,
        "{:?}",
        unchecked_call.diagnostics,
    );
    assert_eq!(unchecked_call.exit_status, CompileExitStatus::Success);
    assert!(unchecked_call.diagnostics.is_empty());
    assert_eq!(
        javascript(&unchecked_call),
        concat!(
            "\"use strict\";\n",
            "var factory;\n",
            "var called = factory() / 2;\n",
        ),
    );
}

#[test]
fn member_names_and_simple_as_types_keep_following_slashes_on_the_division_path() {
    let member_source = concat!(
        "var receiver: any = {};\n",
        "var classValue = receiver.class / 2;\n",
        "var defaultValue = receiver.default / 2;\n",
        "var awaitValue = receiver.await / 2;\n",
        "var yieldValue = receiver.yield / 2;",
    );
    let scanned_members = scan_source(&SourceText::new(
        FileId(13),
        PathBuf::from("member-names.ts"),
        Arc::<str>::from(member_source),
    ));
    assert!(
        scanned_members.diagnostics.is_empty(),
        "{:?}",
        scanned_members.diagnostics,
    );
    assert_eq!(
        scanned_members
            .tokens
            .windows(2)
            .filter_map(|tokens| { (tokens[1].kind == TokenKind::Slash).then_some(tokens[0].kind) })
            .collect::<Vec<_>>(),
        vec![
            TokenKind::Class,
            TokenKind::Default,
            TokenKind::Await,
            TokenKind::Yield,
        ],
    );
    assert!(
        !scanned_members
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::RegularExpressionLiteral)
    );
    let scanned_optional_member = scan_source(&SourceText::new(
        FileId(17),
        PathBuf::from("optional-member-name.ts"),
        Arc::<str>::from("receiver?.class / 2;"),
    ));
    assert!(scanned_optional_member.diagnostics.is_empty());
    assert!(scanned_optional_member.tokens.windows(2).any(|tokens| {
        tokens[0].kind == TokenKind::Class && tokens[1].kind == TokenKind::Slash
    }));
    assert!(
        !scanned_optional_member
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::RegularExpressionLiteral)
    );
    let expected_members = concat!(
        "\"use strict\";\n",
        "var receiver = {};\n",
        "var classValue = receiver.class / 2;\n",
        "var defaultValue = receiver.default / 2;\n",
        "var awaitValue = receiver.await / 2;\n",
        "var yieldValue = receiver.yield / 2;\n",
    );
    for no_check in [false, true] {
        let output = compile("member-names.ts", member_source, options(no_check));
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "noCheck={no_check}: {:?}",
            output.diagnostics,
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(javascript(&output), expected_members);
    }

    let as_type_source = concat!(
        "var voidValue = 4 as void / 2;\n",
        "var awaitType = 4 as await / 2;\n",
        "var yieldType = 4 as yield / 2;",
    );
    let scanned_as_types = scan_source(&SourceText::new(
        FileId(14),
        PathBuf::from("as-types.ts"),
        Arc::<str>::from(as_type_source),
    ));
    assert!(
        scanned_as_types.diagnostics.is_empty(),
        "{:?}",
        scanned_as_types.diagnostics,
    );
    assert_eq!(
        scanned_as_types
            .tokens
            .windows(2)
            .filter_map(|tokens| { (tokens[1].kind == TokenKind::Slash).then_some(tokens[0].kind) })
            .collect::<Vec<_>>(),
        vec![TokenKind::Void, TokenKind::Await, TokenKind::Yield],
    );
    assert!(
        !scanned_as_types
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::RegularExpressionLiteral)
    );

    let unchecked = compile("as-types.ts", as_type_source, options(true));
    assert_eq!(
        unchecked.semantic_completion,
        SemanticCompletion::Complete,
        "{:?}",
        unchecked.diagnostics,
    );
    assert_eq!(unchecked.exit_status, CompileExitStatus::Success);
    assert!(
        unchecked.diagnostics.is_empty(),
        "{:?}",
        unchecked.diagnostics
    );
    assert_eq!(
        javascript(&unchecked),
        concat!(
            "\"use strict\";\n",
            "var voidValue = 4 / 2;\n",
            "var awaitType = 4 / 2;\n",
            "var yieldType = 4 / 2;\n",
        ),
    );
}

#[test]
fn declaration_maps_options_and_global_regexp_collisions_fail_closed() {
    for no_check in [false, true] {
        for mode in [
            "declaration",
            "declarationMap",
            "declarationDir",
            "sourceMap",
            "inlineSourceMap",
        ] {
            let mut compiler_options = options(no_check);
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
            let output = compile("products.ts", "var value = /x/;", compiler_options);
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "mode={mode} noCheck={no_check}: {:?}",
                output.diagnostics
            );
            assert!(output.emitted_files.is_empty(), "mode={mode}");
        }

        let no_lib = compile(
            "no-lib.ts",
            "var value = /x/;",
            CompilerOptions {
                no_lib: true,
                ..options(no_check)
            },
        );
        assert_eq!(no_lib.semantic_completion, SemanticCompletion::Deferred);
        assert!(no_lib.emitted_files.is_empty());
        assert_eq!(codes(&no_lib), vec![2318; 10]);

        for source in [
            "interface RegExp { campaignBrand: string }\nvar value = /x/;",
            "type RegExp = string; var value = /x/;",
            "var RegExp = /x/;",
        ] {
            let output = compile("collision.ts", source, options(no_check));
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "{source}"
            );
            assert!(output.emitted_files.is_empty(), "{source}");
        }
    }
}

#[test]
fn regex_program_results_are_cold_warm_and_root_order_stable() {
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

    let families = [
        vec![
            SourceInput::new("a.ts", Arc::<str>::from(r"var alphaPattern = /\u{41}/gu;")),
            SourceInput::new("b.ts", Arc::<str>::from(r"var betaPattern = /[a-z]+/gi;")),
        ],
        vec![
            SourceInput::new("a.ts", Arc::<str>::from(r"/\u{41}/gu;")),
            SourceInput::new("b.ts", Arc::<str>::from(r"/(#?\d+)|[a-z]/gi;")),
        ],
    ];

    for (family, roots) in families.into_iter().enumerate() {
        for no_check in [false, true] {
            let compiler = Compiler::new();
            let compiler_options = options(no_check);
            let expected = fingerprint(&compiler.compile(roots.clone(), &compiler_options));
            assert_eq!(expected.0, SemanticCompletion::Complete);
            assert_eq!(expected.1, CompileExitStatus::Success);
            for iteration in 0..10 {
                let ordered = if iteration % 2 == 0 {
                    roots.clone()
                } else {
                    roots.iter().rev().cloned().collect()
                };
                assert_eq!(
                    fingerprint(&compiler.compile(ordered, &compiler_options)),
                    expected,
                    "family={family} noCheck={no_check} iteration={iteration}"
                );
            }

            let mixed = compiler.compile(
                vec![
                    SourceInput::new("mixed-a.ts", Arc::<str>::from(r"var value = /x/;")),
                    SourceInput::new("mixed-b.ts", Arc::<str>::from(r"/y/;")),
                ],
                &compiler_options,
            );
            assert_eq!(mixed.semantic_completion, SemanticCompletion::Deferred);
            assert!(mixed.emitted_files.is_empty());
        }
    }
}
