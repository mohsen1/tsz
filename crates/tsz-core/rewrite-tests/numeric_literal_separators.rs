use std::path::PathBuf;
use std::sync::Arc;

use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    ExpressionKind, Literal, NumberLiteral, StatementKind, TokenKind, parse_source, scan_source,
};
use tsz::{
    CompileExitStatus, CompileOutput, Compiler, CompilerOptions, SemanticCompletion, SourceInput,
};

fn options(target: &str, no_check: bool, no_emit: bool) -> CompilerOptions {
    CompilerOptions {
        target: target.to_string(),
        module: "esnext".to_string(),
        no_check,
        no_emit,
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
        .expect("JavaScript product")
        .text
        .as_str()
}

fn declaration(output: &CompileOutput) -> &str {
    output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration product")
        .text
        .as_str()
}

fn assert_complete(output: &CompileOutput) {
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success);
}

fn assert_incomplete(source: &str, no_check: bool, no_emit: bool) {
    let output = compile("case.ts", source, options("es2020", no_check, no_emit));
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Deferred,
        "{source:?}: noCheck={no_check} noEmit={no_emit}"
    );
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(
        output.emitted_files.is_empty(),
        "{source:?}: {:?}",
        output.emitted_files
    );
}

#[test]
fn scanner_stages_valid_number_separator_raw_and_canonical_text() {
    let cases = [
        ("1_000", "1000"),
        (".1_0", "0.1"),
        ("1.1_00_01", "1.10001"),
        ("1e1_0", "10000000000"),
        ("1e-1_0", "1e-10"),
        ("1_2.3_4e5_6", "1.234e+57"),
        ("0b1010_0001_1000_0101", "41349"),
        ("0o7_0", "56"),
        ("0xA0_B0_C0", "10531008"),
        ("9_007_199_254_740_993", "9007199254740992"),
        ("0x20_0000_0000_0001", "9007199254740992"),
        ("1e3_09", "Infinity"),
        ("1e-3_24", "0"),
    ];
    for (raw, canonical) in cases {
        let source = SourceText::new(FileId(3), PathBuf::from("case.ts"), Arc::<str>::from(raw));
        let scanned = scan_source(&source);
        assert!(scanned.diagnostics.is_empty(), "{raw}");
        assert_eq!(scanned.tokens[0].kind, TokenKind::NumericLiteral, "{raw}");
        assert_eq!(scanned.tokens[0].span.start, 0, "{raw}");
        assert_eq!(scanned.tokens[0].span.end as usize, raw.len(), "{raw}");

        let parsed = parse_source(&source);
        assert!(parsed.diagnostics.is_empty(), "{raw}");
        let [statement] = parsed.unit.statements.as_slice() else {
            panic!("one statement for {raw}");
        };
        let StatementKind::Expression(expression) = &statement.kind else {
            panic!("expression statement for {raw}");
        };
        let ExpressionKind::Literal(Literal::Number(NumberLiteral::Separated(literal))) =
            &expression.kind
        else {
            panic!("separated number for {raw}");
        };
        assert_eq!(literal.raw(), raw);
        assert_eq!(literal.canonical(), canonical);
    }
}

#[test]
fn javascript_target_selects_exact_canonical_or_authored_spelling() {
    let source = concat!(
        "1_000;\n",
        ".1_0;\n",
        "1.1_00_01;\n",
        "1e1_0;\n",
        "1e-1_0;\n",
        "1_2.3_4e5_6;\n",
        "0b1010_0001_1000_0101;\n",
        "0o7_0;\n",
        "0xA0_B0_C0;\n",
        "9_007_199_254_740_993;\n",
        "0x20_0000_0000_0001;\n",
        "1e3_09;\n",
        "1e-3_24;",
    );
    let es2020 = compile("case.ts", source, options("es2020", false, false));
    assert_complete(&es2020);
    assert_eq!(
        javascript(&es2020),
        concat!(
            "\"use strict\";\n",
            "1000;\n",
            "0.1;\n",
            "1.10001;\n",
            "10000000000;\n",
            "1e-10;\n",
            "1.234e+57;\n",
            "41349;\n",
            "56;\n",
            "10531008;\n",
            "9007199254740992;\n",
            "9007199254740992;\n",
            "Infinity;\n",
            "0;\n",
        )
    );

    let es2021 = compile("case.ts", source, options("es2021", true, false));
    assert_complete(&es2021);
    assert_eq!(javascript(&es2021), format!("\"use strict\";\n{source}\n"));
}

#[test]
fn member_access_uses_the_exact_decimal_dot_rule_and_radix_quirk() {
    let source = concat!(
        "8_8e4.toString();\n",
        "(1_000).toString();\n",
        "1.2_5.toString();\n",
        "0xF_F.toString();\n",
        "1e3_09.toString();\n",
        "(1_000 as number).toString();\n",
        "((1_000 as number)).toString();",
    );
    let es2020 = compile("member.ts", source, options("es2020", true, false));
    assert_complete(&es2020);
    assert_eq!(
        javascript(&es2020),
        concat!(
            "\"use strict\";\n",
            "880000..toString();\n",
            "(1000).toString();\n",
            "1.25.toString();\n",
            "255.toString();\n",
            "Infinity..toString();\n",
            "1000..toString();\n",
            "1000..toString();\n",
        )
    );

    let es2021 = compile("member.ts", source, options("es2021", true, false));
    assert_complete(&es2021);
    assert_eq!(
        javascript(&es2021),
        concat!(
            "\"use strict\";\n",
            "8_8e4.toString();\n",
            "(1_000).toString();\n",
            "1.2_5.toString();\n",
            "0xF_F.toString();\n",
            "1e3_09.toString();\n",
            "1_000..toString();\n",
            "1_000..toString();\n",
        )
    );
}

#[test]
fn unary_and_declaration_products_use_canonical_numeric_identity() {
    let unary_source = "+0xF_F; -1_000; -1e-3_24;";
    let declaration_source = "export const value = 1_000;";
    for target in ["es2020", "es2021"] {
        let unary = compile("unary.ts", unary_source, options(target, true, false));
        assert_complete(&unary);
        assert_eq!(
            javascript(&unary),
            if target == "es2020" {
                "\"use strict\";\n+255;\n-1000;\n-0;\n"
            } else {
                "\"use strict\";\n+0xF_F;\n-1_000;\n-1e-3_24;\n"
            },
            "{target}"
        );

        let output = compile(
            "values.ts",
            declaration_source,
            CompilerOptions {
                declaration: true,
                ..options(target, false, false)
            },
        );
        assert_complete(&output);
        let expected_javascript = if target == "es2020" {
            "export const value = 1000;\n"
        } else {
            "export const value = 1_000;\n"
        };
        assert_eq!(javascript(&output), expected_javascript, "{target}");
        assert_eq!(
            declaration(&output),
            "export declare const value = 1000;\n",
            "{target}"
        );
    }
}

#[test]
fn property_type_and_trivia_member_hosts_fail_closed_before_semantics_or_emit() {
    let sources = [
        "({1_0: 1});",
        "class Item { 1_0 = 1; }",
        "interface Shape { 1_0: string; }",
        "export interface Shape { [1_0]: string; }",
        "export type Shape = { [1_0 + 2]: string };",
        "type Value = 1_0;",
        "8_8e4 .toString();",
        "8_8e4/*gap*/.toString();",
        "1_0value;",
        "1_0?.toString();",
        "1_0n;",
    ];
    for source in sources {
        for no_check in [false, true] {
            for no_emit in [false, true] {
                assert_incomplete(source, no_check, no_emit);
            }
        }
    }

    let mut service = LanguageService::new(options("es2020", false, false));
    service.open("service.ts", Arc::<str>::from("type Measure = 1_0;"));
    assert!(service.quick_info("service.ts", 7).is_none());
    let output = service.compile();
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    for source in ["const obj = { 1_0: 1 };", "const n = 1_0n;"] {
        let mut service = LanguageService::new(options("es2020", false, false));
        service.open("service.ts", Arc::<str>::from(source));
        assert!(service.quick_info("service.ts", 6).is_none(), "{source}");
        let output = service.compile();
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }

    for roots in [
        vec![
            SourceInput::new("safe.ts", Arc::<str>::from("1_000;")),
            SourceInput::new("key.ts", Arc::<str>::from("({1_0: 1});")),
        ],
        vec![
            SourceInput::new("key.ts", Arc::<str>::from("({1_0: 1});")),
            SourceInput::new("safe.ts", Arc::<str>::from("1_000;")),
        ],
    ] {
        let output = Compiler::new().compile(roots, &options("es2020", false, false));
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(
            output.stats.types > 0,
            "the safe sibling remains independently checkable"
        );
        assert_eq!(output.emitted_files.len(), 1);
        assert_eq!(output.emitted_files[0].path, PathBuf::from("safe.js"));
        assert_eq!(output.emitted_files[0].text, "\"use strict\";\n1000;\n");
    }
}

#[test]
fn plain_bigint_and_invalid_separator_families_do_not_enter_the_new_variant() {
    let sources = [
        ("1000", false),
        ("0xFF", false),
        ("1_0n", false),
        ("1__0", false),
        ("0x_FF", false),
    ];
    for (raw, separated) in sources {
        let source = SourceText::new(
            FileId(9),
            PathBuf::from("boundary.ts"),
            Arc::<str>::from(raw),
        );
        let parsed = parse_source(&source);
        let is_separated = parsed.unit.statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Expression(expression)
                    if matches!(
                        &expression.kind,
                        ExpressionKind::Literal(Literal::Number(NumberLiteral::Separated(_)))
                    )
            )
        });
        assert_eq!(is_separated, separated, "{raw}");
    }
}

#[test]
fn completion_modes_and_root_order_are_stable_for_owned_separator_literals() {
    for no_check in [false, true] {
        for no_emit in [false, true] {
            let output = compile(
                "mode.ts",
                "1_000; 0xF_F;",
                options("es2020", no_check, no_emit),
            );
            assert_complete(&output);
            assert_eq!(output.emitted_files.is_empty(), no_emit);
            if !no_emit {
                assert_eq!(javascript(&output), "\"use strict\";\n1000;\n255;\n");
            }
        }
    }

    let compile_roots =
        |roots: Vec<SourceInput>| Compiler::new().compile(roots, &options("es2020", true, false));
    let forward = compile_roots(vec![
        SourceInput::new("zeta.ts", Arc::<str>::from("1_000;")),
        SourceInput::new("alpha.ts", Arc::<str>::from("0xF_F;")),
    ]);
    let reverse = compile_roots(vec![
        SourceInput::new("alpha.ts", Arc::<str>::from("0xF_F;")),
        SourceInput::new("zeta.ts", Arc::<str>::from("1_000;")),
    ]);
    assert_complete(&forward);
    assert_complete(&reverse);
    let products = |output: &CompileOutput| {
        output
            .emitted_files
            .iter()
            .map(|file| (file.path.clone(), file.text.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(products(&forward), products(&reverse));
}
