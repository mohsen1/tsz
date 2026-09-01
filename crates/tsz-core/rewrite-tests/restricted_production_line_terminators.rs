use std::path::PathBuf;
use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    ExpressionKind, JumpStatement, Statement, StatementKind, TokenKind, parse_source, scan_source,
};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

const LINE_TERMINATORS: [(&str, &str); 5] = [
    ("lf", "\n"),
    ("cr", "\r"),
    ("crlf", "\r\n"),
    ("line-separator", "\u{2028}"),
    ("paragraph-separator", "\u{2029}"),
];

fn source(path: &str, text: &str) -> SourceText {
    SourceText::new(FileId(0), PathBuf::from(path), Arc::<str>::from(text))
}

fn parse(path: &str, text: &str) -> (SourceText, tsz::syntax::ParseOutput) {
    let source = source(path, text);
    let parsed = parse_source(&source);
    (source, parsed)
}

fn compile(path: &str, text: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(path, Arc::<str>::from(text))],
        &CompilerOptions {
            no_emit: true,
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    )
}

fn jump(statement: &Statement) -> &JumpStatement {
    match &statement.kind {
        StatementKind::Break(jump) | StatementKind::Continue(jump) => jump,
        kind => panic!("jump statement expected, got {kind:?}"),
    }
}

fn assert_restricted_label(
    keyword: &str,
    separator_name: &str,
    separator: &str,
    inside_comment: bool,
) {
    let following = format!("after_{keyword}_{}", separator_name.replace('-', "_"));
    let gap = if inside_comment {
        format!("/*before{separator}after*/")
    } else {
        separator.to_string()
    };
    let source_text = format!("{keyword}{gap}{following};");
    let (source, parsed) = parse("restricted.ts", &source_text);

    assert_eq!(
        parsed.diagnostics,
        [],
        "{keyword} with {separator_name}: {:#?}",
        parsed.diagnostics,
    );
    let [jump_statement, reference_statement] = parsed.unit.statements.as_slice() else {
        panic!(
            "{keyword} with {separator_name} must leave the following identifier as its own statement: {:#?}",
            parsed.unit.statements,
        )
    };
    assert_eq!(source.slice(jump_statement.span), keyword);
    assert_eq!(jump(jump_statement).label, None);

    let StatementKind::Expression(reference) = &reference_statement.kind else {
        panic!("following identifier must be an expression statement")
    };
    let ExpressionKind::Identifier { name, .. } = &reference.kind else {
        panic!("following statement must retain its identifier reference")
    };
    assert_eq!(name, &following);
}

#[test]
fn every_ecmascript_line_terminator_restricts_break_and_continue_labels() {
    for keyword in ["break", "continue"] {
        for (name, terminator) in LINE_TERMINATORS {
            assert_restricted_label(keyword, name, terminator, false);
        }
    }
}

#[test]
fn scanner_separates_unicode_trivia_without_rejecting_unicode_identifiers() {
    for (name, separator) in [
        ("line separator", "\u{2028}"),
        ("paragraph separator", "\u{2029}"),
        ("next-line control", "\u{0085}"),
        ("zero-width space", "\u{200b}"),
    ] {
        let text = format!("break{separator}renamed_target");
        let source = source("scanner.ts", &text);
        let scanned = scan_source(&source);
        assert_eq!(
            scanned.diagnostics,
            [],
            "{name}: {:#?}",
            scanned.diagnostics,
        );
        assert_eq!(
            scanned
                .tokens
                .iter()
                .map(|token| token.kind)
                .collect::<Vec<_>>(),
            [
                TokenKind::Break,
                TokenKind::Identifier,
                TokenKind::EndOfFile
            ],
            "{name}",
        );
        assert_eq!(source.slice(scanned.tokens[0].span), "break");
        assert_eq!(source.slice(scanned.tokens[1].span), "renamed_target");
    }

    let source = source("unicode-identifiers.ts", "const café = π;");
    let scanned = scan_source(&source);
    assert_eq!(scanned.diagnostics, [], "{:#?}", scanned.diagnostics);
    assert_eq!(source.slice(scanned.tokens[1].span), "café");
    assert_eq!(source.slice(scanned.tokens[3].span), "π");
}

#[test]
fn line_terminators_inside_block_comments_also_restrict_jump_labels() {
    for keyword in ["break", "continue"] {
        for (name, terminator) in LINE_TERMINATORS {
            assert_restricted_label(keyword, name, terminator, true);
        }
    }
}

#[test]
fn same_line_trivia_and_u0085_keep_the_authored_label() {
    for keyword in ["break", "continue"] {
        for (name, separator) in [
            ("space", " "),
            ("block comment", "/*same line*/"),
            ("next-line control", "\u{0085}"),
            ("zero-width space", "\u{200b}"),
        ] {
            let source_text = format!("{keyword}{separator}renamed_target;");
            let (source, parsed) = parse("same-line.ts", &source_text);

            assert_eq!(
                parsed.diagnostics,
                [],
                "{keyword} with {name}: {:#?}",
                parsed.diagnostics,
            );
            let [statement] = parsed.unit.statements.as_slice() else {
                panic!("{keyword} with {name} must remain one labeled jump")
            };
            let jump = jump(statement);
            assert_eq!(jump.label.as_deref(), Some("renamed_target"));
            assert_eq!(
                source.slice(jump.label_span.expect("authored label span")),
                "renamed_target",
            );
        }
    }
}

#[test]
fn zero_width_space_keeps_break_label_on_same_line_for_semantic_diagnostic() {
    // U+200B is TypeScript single-line whitespace, not an ECMAScript line
    // terminator. The parser must therefore retain `target` as the authored
    // label; semantic checking then reports the pinned out-of-scope TS1116,
    // rather than a parser TS1109 or a missing-name diagnostic.
    let source = "break\u{200b}target;";
    let output = compile("break-target.ts", source);

    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!(
            "one semantic diagnostic expected: {:#?}",
            output.diagnostics
        )
    };
    assert_eq!(diagnostic.code, 1116);
    assert_eq!(diagnostic.file, "break-target.ts");
    assert_eq!(diagnostic.category, DiagnosticCategory::Error);
    assert_eq!(
        diagnostic.message_text,
        "A 'break' statement can only jump to a label of an enclosing statement."
    );
    assert_eq!(diagnostic.start, 0);
    assert_eq!(diagnostic.length, "break".encode_utf16().count() as u32);
}

#[test]
fn jump_diagnostics_use_the_nearest_represented_semantic_context() {
    for (path, source, keyword, code, message) in [
        (
            "plain-break.ts",
            "break;",
            "break",
            1105,
            "A 'break' statement can only be used within an enclosing iteration or switch statement.",
        ),
        (
            "plain-continue.ts",
            "continue;",
            "continue",
            1104,
            "A 'continue' statement can only be used within an enclosing iteration statement.",
        ),
        (
            "labeled-continue.ts",
            "continue\u{200b}renamed_target;",
            "continue",
            1115,
            "A 'continue' statement can only jump to a label of an enclosing iteration statement.",
        ),
        (
            "switch-labeled-break.ts",
            "switch (1) { default: break\u{200b}renamed_target; }",
            "break",
            1116,
            "A 'break' statement can only jump to a label of an enclosing statement.",
        ),
        (
            "switch-continue.ts",
            "switch (1) { default: continue; }",
            "continue",
            1104,
            "A 'continue' statement can only be used within an enclosing iteration statement.",
        ),
    ] {
        let output = compile(path, source);
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("{path}: one diagnostic expected: {:#?}", output.diagnostics)
        };
        let byte_start = source.find(keyword).expect("authored jump keyword");
        assert_eq!(diagnostic.file, path, "{path}");
        assert_eq!(diagnostic.code, code, "{path}");
        assert_eq!(diagnostic.category, DiagnosticCategory::Error, "{path}");
        assert_eq!(
            diagnostic.start,
            source[..byte_start].encode_utf16().count() as u32,
            "{path}",
        );
        assert_eq!(
            diagnostic.length,
            keyword.encode_utf16().count() as u32,
            "{path}",
        );
        assert_eq!(diagnostic.message_text, message, "{path}");
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
    }

    for source in [
        "switch (1) { default: break; }",
        "function nested(): void { switch (1) { case 1: { break; } } }",
    ] {
        let output = compile("allowed-switch.ts", source);
        assert_eq!(
            output.diagnostics,
            [],
            "{source}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }
}

#[test]
fn recovered_loop_ancestors_fence_jump_legality_until_loops_are_represented() {
    let output = compile(
        "recovered-loop.ts",
        "declare const items: { value: number }[]; for (const { value } of items) { break; continue; }",
    );
    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| !matches!(diagnostic.code, 1104 | 1105 | 1115 | 1116)),
        "{:#?}",
        output.diagnostics,
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn separated_identifier_is_checked_as_an_independent_reference() {
    let source = "switch (1) { default: break\u{2028}renamed_missing; }";
    let output = Compiler::new().compile(
        vec![SourceInput::new("diagnostic.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );

    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!(
            "one missing-name diagnostic expected: {:#?}",
            output.diagnostics
        )
    };
    assert_eq!(diagnostic.code, 2304);
    let byte_start = source.find("renamed_missing").expect("reference span");
    assert_eq!(
        (diagnostic.start, diagnostic.length),
        (
            source[..byte_start].encode_utf16().count() as u32,
            "renamed_missing".len() as u32,
        ),
    );
    assert_eq!(
        diagnostic.message_text,
        "Cannot find name 'renamed_missing'."
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped,
    );
}

#[test]
fn javascript_emit_keeps_restricted_jump_and_following_statement_separate() {
    let source = "break\u{2028}after_break;continue\u{2029}after_continue;";
    let output = Compiler::new().compile(
        vec![SourceInput::new("emit.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_check: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );

    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    let [javascript] = output.emitted_files.as_slice() else {
        panic!(
            "one JavaScript output expected: {:#?}",
            output.emitted_files
        )
    };
    assert_eq!(
        javascript.text,
        concat!(
            "\"use strict\";\n",
            "break;\n",
            "after_break;\n",
            "continue;\n",
            "after_continue;\n",
        ),
    );
}
