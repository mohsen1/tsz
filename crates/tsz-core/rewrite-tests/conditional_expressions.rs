use std::path::PathBuf;
use std::sync::Arc;

use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    BinaryOperator, Expression, ExpressionKind, Literal, StatementKind, parse_source,
};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn parsed_expression(source: &str) -> Expression {
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("conditional.ts"),
        Arc::<str>::from(source),
    ));
    assert_eq!(
        parsed.diagnostics,
        [],
        "{source}: {:#?}",
        parsed.diagnostics
    );
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("one expression statement expected: {source}")
    };
    let StatementKind::Expression(expression) = &statement.kind else {
        panic!("expression statement expected: {source}")
    };
    expression.clone()
}

fn expression_shape(expression: &Expression) -> String {
    match &expression.kind {
        ExpressionKind::Identifier { name, .. } => name.clone(),
        ExpressionKind::Literal(Literal::Number(number)) => number.raw().to_string(),
        ExpressionKind::Conditional {
            condition,
            when_true,
            when_false,
            ..
        } => format!(
            "({} ? {} : {})",
            expression_shape(condition),
            expression_shape(when_true),
            expression_shape(when_false),
        ),
        ExpressionKind::Binary {
            left,
            operator: BinaryOperator::Comma,
            right,
            ..
        } => format!("({}, {})", expression_shape(left), expression_shape(right)),
        ExpressionKind::Parenthesized(inner) => format!("({})", expression_shape(inner)),
        kind => panic!("unexpected expression: {kind:?}"),
    }
}

#[test]
fn comma_expression_is_lowest_precedence_left_associative_and_retains_wrappers() {
    assert_eq!(
        expression_shape(&parsed_expression("alpha, beta, gamma;")),
        "((alpha, beta), gamma)",
    );
    assert_eq!(
        expression_shape(&parsed_expression("flag ? yes : (left, right);")),
        "(flag ? yes : ((left, right)))",
    );
    assert_eq!(
        expression_shape(&parsed_expression("(left, right) ? yes : no;")),
        "(((left, right)) ? yes : no)",
    );
}

#[test]
fn commas_in_generic_calls_and_arrays_remain_list_separators() {
    let source = "consume<number>((discarded, chosen), fallback); [first, second];";
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("lists.ts"),
        Arc::<str>::from(source),
    ));
    assert_eq!(parsed.diagnostics, [], "{:#?}", parsed.diagnostics);
    let [call, array] = parsed.unit.statements.as_slice() else {
        panic!(
            "two expression statements expected: {:#?}",
            parsed.unit.statements
        )
    };
    let StatementKind::Expression(Expression {
        kind: ExpressionKind::Call { arguments, .. },
        ..
    }) = &call.kind
    else {
        panic!("generic call expected: {call:#?}")
    };
    assert_eq!(arguments.len(), 2);
    assert!(matches!(
        arguments[0].kind,
        ExpressionKind::Parenthesized(_)
    ));
    let StatementKind::Expression(Expression {
        kind: ExpressionKind::Array(elements),
        ..
    }) = &array.kind
    else {
        panic!("array expected: {array:#?}")
    };
    assert_eq!(elements.len(), 2);
}

#[test]
fn comma_expression_witness_keeps_independent_diagnostics_while_global_this_defers() {
    // TypeScript 7 reports the seven missing-name diagnostics below plus
    // TS7017 for `this.R` and `this.A`. TSZ cannot claim either property
    // diagnostic until the program-global object shape owns script `this`.
    // The nonclaim graduates when that shape is supplied to the checker
    // session; comma and conditional traversal must still retain independent
    // diagnostics from every operand and branch in the meantime.
    let source = "(a=this.R[c])?a.JW||(a.e5(this,c),a.JW=_.l):this.A";
    let output = Compiler::new().compile(
        vec![SourceInput::new("witness.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (2304, 1, 1, "Cannot find name 'a'."),
            (2304, 10, 1, "Cannot find name 'c'."),
            (2304, 14, 1, "Cannot find name 'a'."),
            (2304, 21, 1, "Cannot find name 'a'."),
            (2304, 31, 1, "Cannot find name 'c'."),
            (2304, 34, 1, "Cannot find name 'a'."),
            (2304, 39, 1, "Cannot find name '_'."),
        ],
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn comma_expression_global_this_nonclaim_is_repeatable_and_root_order_independent() {
    let dependent = (
        "a-dependent.ts",
        "((renamed=this.Table[key])?renamed.pick||(renamed.call(this,key),renamed.pick=space.value):this.fallback)",
    );
    let independent = ("z-independent.ts", "const kept:MissingSibling=1;");
    for roots in [
        [dependent, independent],
        [dependent, independent],
        [independent, dependent],
    ] {
        let output = Compiler::new().compile(
            roots
                .into_iter()
                .map(|(path, source)| SourceInput::new(path, Arc::<str>::from(source)))
                .collect(),
            &CompilerOptions {
                no_emit: true,
                target: "es2015".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == 2304),
            "{:#?}",
            output.diagnostics,
        );
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.file.as_str(),
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.message_text.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![
                ("a-dependent.ts", 2, 7, "Cannot find name 'renamed'."),
                ("a-dependent.ts", 21, 3, "Cannot find name 'key'."),
                ("a-dependent.ts", 27, 7, "Cannot find name 'renamed'."),
                ("a-dependent.ts", 42, 7, "Cannot find name 'renamed'."),
                ("a-dependent.ts", 60, 3, "Cannot find name 'key'."),
                ("a-dependent.ts", 65, 7, "Cannot find name 'renamed'."),
                ("a-dependent.ts", 78, 5, "Cannot find name 'space'."),
                (
                    "z-independent.ts",
                    11,
                    14,
                    "Cannot find name 'MissingSibling'.",
                ),
            ],
        );
    }

    let unchecked = Compiler::new().compile(
        vec![SourceInput::new(dependent.0, Arc::<str>::from(dependent.1))],
        &CompilerOptions {
            no_check: true,
            no_emit: true,
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(unchecked.diagnostics, []);
    assert_eq!(unchecked.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(unchecked.exit_status, CompileExitStatus::Success);
}

#[test]
fn comma_expression_type_and_emit_are_owned_by_the_right_operand() {
    let source = concat!(
        "declare const ignored: number; declare const selected: string;",
        "const value: number = (ignored, selected);",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new("typed.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("one right-operand relation diagnostic expected: {output:#?}")
    };
    assert_eq!(
        (
            diagnostic.code,
            diagnostic.start,
            diagnostic.length,
            diagnostic.message_text.as_str(),
        ),
        (
            2322,
            68,
            5,
            "Type 'string' is not assignable to type 'number'."
        ),
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);

    let emitted = Compiler::new().compile(
        vec![SourceInput::new(
            "emit.ts",
            Arc::<str>::from("const chosen = (left, right); take(left, right);"),
        )],
        &CompilerOptions {
            no_check: true,
            target: "es2022".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(emitted.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(emitted.emitted_files.len(), 1);
    assert_eq!(
        emitted.emitted_files[0].text,
        "\"use strict\";\nconst chosen = (left, right);\ntake(left, right);\n"
    );
}

#[test]
fn global_this_projection_does_not_escape_its_script_owner() {
    for (path, source) in [
        ("module.ts", "export {}; this.missing;"),
        (
            "function.ts",
            "function ordinary() { return this.missing; }",
        ),
        (
            "class.ts",
            "class Owner { method() { return this.missing; } }",
        ),
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(path, Arc::<str>::from(source))],
            &CompilerOptions {
                no_emit: true,
                ..CompilerOptions::default()
            },
        );
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != 7017),
            "{path}: {:#?}",
            output.diagnostics,
        );
    }
}

#[test]
fn trailing_comma_expression_keeps_parser_recovery_explicit() {
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("malformed-comma.ts"),
        Arc::<str>::from("(renamed,);"),
    ));
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
            .collect::<Vec<_>>(),
        [(1109, 9, 1)],
    );
}

#[test]
fn conditional_expression_is_one_authored_tree() {
    let expression = parsed_expression("flag ? 1 : 2;");
    assert_eq!(expression_shape(&expression), "(flag ? 1 : 2)");
    let ExpressionKind::Conditional {
        question_span,
        colon_span,
        ..
    } = expression.kind
    else {
        unreachable!()
    };
    assert_eq!((question_span.start, question_span.len()), (5, 1));
    assert_eq!(
        colon_span.map(|span| (span.start, span.len())),
        Some((9, 1))
    );
}

#[test]
fn conditional_expression_is_right_associative_and_parentheses_are_retained() {
    assert_eq!(
        expression_shape(&parsed_expression("a ? b : c ? 1 : 2;")),
        "(a ? b : (c ? 1 : 2))",
    );
    assert_eq!(
        expression_shape(&parsed_expression("(a ? b : c) ? 1 : 2;")),
        "(((a ? b : c)) ? 1 : 2)",
    );
}

#[test]
fn malformed_conditional_branches_report_the_delimiter_owned_diagnostic() {
    let cases = [
        ("flag ? : 1;", 1109, 7, 1, "Expression expected."),
        ("flag ? 1 2;", 1005, 9, 1, "':' expected."),
        ("flag ? 1 : ;", 1109, 11, 1, "Expression expected."),
    ];
    for (source, code, start, length, message) in cases {
        let parsed = parse_source(&SourceText::new(
            FileId(0),
            PathBuf::from("conditional.ts"),
            Arc::<str>::from(source),
        ));
        let identities = parsed
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
        let [(actual_code, actual_start, actual_length, actual_message)] = identities.as_slice()
        else {
            panic!("{source}: {:#?}", parsed.diagnostics)
        };
        assert_eq!(
            (*actual_code, *actual_start, *actual_length, *actual_message),
            (code, start, length, message),
            "{source}",
        );
    }
}

#[test]
fn missing_colon_retains_the_following_numeric_statement() {
    let source = "flag ? 1 2;";
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("conditional.ts"),
        Arc::<str>::from(source),
    ));
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        [(1005, 9, 1, "':' expected.")],
    );
    let [conditional, following] = parsed.unit.statements.as_slice() else {
        panic!("missing-colon recovery must retain the following statement")
    };
    let StatementKind::Expression(Expression {
        kind:
            ExpressionKind::Conditional {
                colon_span,
                when_false,
                ..
            },
        ..
    }) = &conditional.kind
    else {
        panic!("conditional statement expected: {conditional:#?}")
    };
    assert_eq!(*colon_span, None);
    assert!(matches!(when_false.kind, ExpressionKind::Missing));
    assert_eq!((when_false.span.start, when_false.span.end), (8, 8));
    let StatementKind::Expression(following) = &following.kind else {
        panic!("following expression statement expected: {following:#?}")
    };
    assert!(matches!(
        &following.kind,
        ExpressionKind::Literal(Literal::Number(number)) if number.raw() == "2"
    ));
    assert_eq!((following.span.start, following.span.end), (9, 10));
}

#[test]
fn missing_colon_retains_identifier_and_declaration_followers() {
    for (source, follower, declaration) in [
        ("flag ? yes no;", "no", false),
        ("flag ? yes const next = 2;", "const", true),
    ] {
        let parsed = parse_source(&SourceText::new(
            FileId(0),
            PathBuf::from("conditional.ts"),
            Arc::<str>::from(source),
        ));
        let start = source.find(follower).expect("follower") as u32;
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
                .collect::<Vec<_>>(),
            [(1005, start, follower.len() as u32)],
            "{source}",
        );
        let [conditional, following] = parsed.unit.statements.as_slice() else {
            panic!(
                "two statements expected: {source}: {:#?}",
                parsed.unit.statements
            )
        };
        let StatementKind::Expression(Expression {
            kind: ExpressionKind::Conditional { when_false, .. },
            ..
        }) = &conditional.kind
        else {
            panic!("conditional statement expected: {source}: {conditional:#?}")
        };
        assert!(matches!(when_false.kind, ExpressionKind::Missing));
        assert!(when_false.span.is_empty());
        assert_eq!(following.span.start, start, "{source}");
        assert_eq!(
            matches!(following.kind, StatementKind::Variable(_)),
            declaration,
            "{source}: {following:#?}",
        );
    }
}

#[test]
fn nested_conditional_missing_colon_leaves_the_call_comma_to_the_argument_list() {
    let source = "consume(flag ? inner ? 1 : 2, sibling);";
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("conditional.ts"),
        Arc::<str>::from(source),
    ));
    let comma = source.find(',').expect("argument comma") as u32;
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
            .collect::<Vec<_>>(),
        [(1005, comma, 1)],
    );
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("one call statement expected: {:#?}", parsed.unit.statements)
    };
    let StatementKind::Expression(Expression {
        kind: ExpressionKind::Call { arguments, .. },
        ..
    }) = &statement.kind
    else {
        panic!("call statement expected: {statement:#?}")
    };
    let [conditional, sibling] = arguments.as_slice() else {
        panic!("the call comma must retain two arguments: {arguments:#?}")
    };
    let ExpressionKind::Conditional {
        when_true,
        when_false,
        ..
    } = &conditional.kind
    else {
        panic!("outer conditional expected: {conditional:#?}")
    };
    assert!(matches!(when_true.kind, ExpressionKind::Conditional { .. }));
    assert!(matches!(when_false.kind, ExpressionKind::Missing));
    assert_eq!((when_false.span.start, when_false.span.end), (comma, comma));
    assert!(matches!(
        &sibling.kind,
        ExpressionKind::Identifier { name, .. } if name == "sibling"
    ));
}

#[test]
fn malformed_conditional_javascript_is_nonclaimed_across_products_and_hosts() {
    let cases = [
        (
            "export const recovered = flag ? : 1;",
            1109,
            32,
            "Expression expected.",
        ),
        (
            "export const recovered = flag ? 1 : ;",
            1109,
            36,
            "Expression expected.",
        ),
        (
            "export const recovered = flag ? 1 2;",
            1005,
            34,
            "':' expected.",
        ),
        (
            "export const recovered = consume(flag ? : 1);",
            1109,
            40,
            "Expression expected.",
        ),
        (
            "export const recovered = [flag ? 1 : ];",
            1109,
            37,
            "Expression expected.",
        ),
        (
            "export const recovered = { value: flag ? : 1 };",
            1109,
            41,
            "Expression expected.",
        ),
    ];
    let stable = SourceInput::new(
        "stable.ts",
        Arc::<str>::from("export const stable: number = 1;"),
    );
    for (source, code, start, message) in cases {
        for module in ["esnext", "commonjs"] {
            for no_check in [false, true] {
                for no_emit_on_error in [false, true] {
                    for reversed in [false, true] {
                        let recovered = SourceInput::new("recovered.ts", Arc::<str>::from(source));
                        let roots = if reversed {
                            vec![stable.clone(), recovered]
                        } else {
                            vec![recovered, stable.clone()]
                        };
                        let output = Compiler::new().compile(
                            roots,
                            &CompilerOptions {
                                no_check,
                                no_emit_on_error,
                                target: "es2022".to_string(),
                                module: module.to_string(),
                                ..CompilerOptions::default()
                            },
                        );
                        assert_eq!(
                            output
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
                            [("recovered.ts", code, start, 1, message)],
                            "{source} module={module} noCheck={no_check} noEmitOnError={no_emit_on_error} reversed={reversed}",
                        );
                        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
                        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
                        let emitted = output
                            .emitted_files
                            .iter()
                            .map(|file| (file.path.as_path(), file.text.as_str()))
                            .collect::<Vec<_>>();
                        if no_emit_on_error {
                            assert!(emitted.is_empty());
                        } else {
                            let expected = if module == "commonjs" {
                                concat!(
                                    "\"use strict\";\n",
                                    "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                                    "exports.stable = void 0;\n",
                                    "exports.stable = 1;\n",
                                )
                            } else {
                                "export const stable = 1;\n"
                            };
                            assert_eq!(emitted, [(std::path::Path::new("stable.js"), expected)]);
                        }
                    }
                }
            }
        }
    }
}
