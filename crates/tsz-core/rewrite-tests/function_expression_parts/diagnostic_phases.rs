use super::*;

fn diagnostic_identities(
    diagnostics: &[tsz::diagnostics::Diagnostic],
) -> Vec<(u32, u32, u32, String)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.clone(),
            )
        })
        .collect()
}

fn aggregate_product_identities(
    source: &str,
    semantic: &[tsz::diagnostics::Diagnostic],
) -> Vec<(u32, u32, u32, String)> {
    let mut aggregate = parse(source).diagnostics;
    aggregate.extend_from_slice(semantic);
    let mut identities = aggregate
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.clone(),
            )
        })
        .collect::<Vec<_>>();
    identities.sort_by(|left, right| {
        (left.1, left.2, left.0, &left.3).cmp(&(right.1, right.2, right.0, &right.3))
    });
    identities.dedup();
    identities
}

#[test]
fn recovered_function_header_blocks_emit_but_body_recovery_keeps_siblings_visible() {
    let malformed = compile("const value = function named(@) { };", false, true);
    assert!(malformed.emitted_files.is_empty());
    assert_eq!(malformed.semantic_completion, SemanticCompletion::Deferred);

    let source = concat!(
        "const value = function named(input: number): number { const broken = ; return input; };\n",
        "const independent: MissingBodySibling = 1;\n",
    );
    let emit = compile(source, false, true);
    assert!(emit.emitted_files.is_empty());
    assert_eq!(emit.semantic_completion, SemanticCompletion::Deferred);
    let service = language_service(source);
    let semantic = service.semantic_diagnostics("function-expression.ts");
    assert!(semantic.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == 2304
            && diagnostic.start == source.find("MissingBodySibling").unwrap() as u32
    }));
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
    let output = service.compile();
    assert_eq!(
        diagnostic_identities(&output.diagnostics),
        aggregate_product_identities(source, &semantic.diagnostics),
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn authored_return_mismatch_survives_a_deferred_flow_host() {
    let source = concat!(
        "let subject: string | number = 0;\n",
        "switch (subject.) { default: break; }\n",
        "const nested = function self(input: number): number { return 'bad'; };\n",
    );
    let service = language_service(source);
    let semantic = service.semantic_diagnostics("function-expression.ts");
    assert!(
        semantic
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == 2322)
    );
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
    let output = service.compile();
    assert_eq!(
        diagnostic_identities(&output.diagnostics),
        aggregate_product_identities(source, &semantic.diagnostics),
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn parenthesized_arrow_certainty_owns_missing_arrow_tokens_without_false_heads() {
    for (path, source) in [
        ("case.ts", "const value = () { return 1; }"),
        ("case.ts", "const value = (named: Cedar) { return named; }"),
        ("case.ts", "const value = (...renamed) { return renamed; }"),
        (
            "case.ts",
            "const value = (public renamed) { return renamed; }",
        ),
        ("case.ts", "const value = (named?: Cedar) { return named; }"),
        ("case.ts", "const value = (named) { return named; }"),
        ("case.ts", "const value = (left, right) { return left; }"),
        ("case.ts", "const value = ({ named }) { return named; }"),
        ("case.ts", "const value = <Cedar>() { return 1; }"),
        ("case.tsx", "const value = <Cedar,>() { return 1; }"),
        ("case.ts", "const value = (named: Cedar);"),
    ] {
        let parsed = parse_path(path, source);
        assert!(
            matches!(
                &variable_initializer(&parsed).kind,
                ExpressionKind::FunctionLike(function)
                    if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
            ),
            "{path}: {source}: {:#?}",
            parsed.diagnostics,
        );
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![1005],
            "{path}: {source}: {:#?}",
            parsed.diagnostics,
        );
    }

    for source in [
        "const value = (named);",
        "const value = (named, changed);",
        "const value = (named, changed + other) => named;",
        "const value = (named, 1) => named;",
        "const value = (named, const) => named;",
        "const value = (named, const changed) => named;",
        "const value = (named, default changed) => named;",
        "const value = (named, in) => named;",
        "const value = (named, in\nchanged) => named;",
        "const value = <Cedar>(named + changed) => named;",
        "const value = <Cedar>(1) { return 1; }",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(
            !matches!(
                &variable_initializer(&parsed).kind,
                ExpressionKind::FunctionLike(_)
            ),
            "{source}: {:#?}",
            parsed.diagnostics,
        );
    }

    let contextual_name = parse_path("case.ts", "const value = (named, public) => named;");
    assert!(matches!(
        &variable_initializer(&contextual_name).kind,
        ExpressionKind::FunctionLike(function)
            if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
    ));
    for source in [
        "const value = (named, in changed) => named;",
        "const value = (named, export changed) => named;",
        "const value = (named, export\nchanged) => named;",
        "const value = (named, static changed) => named;",
        "const value = (named, static\nchanged) => named;",
        "const value = (named, public\nchanged) => named;",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(
            matches!(
                &variable_initializer(&parsed).kind,
                ExpressionKind::FunctionLike(function)
                    if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
            ),
            "{source}: {:#?}",
            parsed.diagnostics,
        );
    }

    let recovered_separator = parse_path(
        "case.ts",
        "const value = (named, public\nchanged) => named;",
    );
    assert_eq!(
        recovered_separator
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![1005],
    );
    for source in [
        "const value = (named, export\nchanged) => named;",
        "const value = (named, static\nchanged) => named;",
    ] {
        assert!(
            parse_path("case.ts", source).diagnostics.is_empty(),
            "{source}"
        );
    }

    for source in [
        "const value = (named, changed third) => named;",
        "const value = <Cedar>(named changed) => named;",
    ] {
        let parsed = parse_path("case.ts", source);
        assert!(matches!(
            &variable_initializer(&parsed).kind,
            ExpressionKind::FunctionLike(function)
                if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
        ));
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == 1005)
        );
    }

    for (source, function_count) in [
        (
            concat!(
                "declare const consume: (...values: any[]) => void;",
                "consume((): => 1, (renamed) { return renamed; }, MissingSameCall);",
            ),
            2,
        ),
        (
            concat!(
                "declare const consume: (...values: any[]) => void;",
                "consume(<Cedar,>(): => 1, (renamed) { return renamed; }, MissingSameCall);",
            ),
            1,
        ),
    ] {
        let parsed = parse_path("case.ts", source);
        let StatementKind::Expression(Expression {
            kind: ExpressionKind::Call { arguments, .. },
            ..
        }) = &parsed.unit.statements[1].kind
        else {
            panic!("expected a call expression: {:#?}", parsed.unit.statements);
        };
        assert_eq!(
            arguments
                .iter()
                .filter(|argument| matches!(argument.kind, ExpressionKind::FunctionLike(_)))
                .count(),
            function_count,
            "{source}: {arguments:#?}",
        );

        let service = language_service(source);
        let semantic = service.semantic_diagnostics("function-expression.ts");
        let missing_names = semantic
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 2304)
            .map(|diagnostic| diagnostic.start)
            .collect::<Vec<_>>();
        assert_eq!(
            missing_names,
            vec![source.find("MissingSameCall").unwrap() as u32],
            "{source}: {:#?}",
            semantic.diagnostics,
        );
        assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
        let output = service.compile();
        assert_eq!(
            diagnostic_identities(&output.diagnostics),
            aggregate_product_identities(source, &semantic.diagnostics),
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn unowned_function_expression_modifiers_withhold_products_and_name_fallout() {
    for source in [
        "export const value = async function changed() { };",
        "export const value = function* changed() { };",
        "export const value = async function* changed() { };",
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "function-expression.ts",
                Arc::<str>::from(source),
            )],
            &CompilerOptions {
                target: "es2022".to_string(),
                declaration: true,
                no_check: true,
                no_emit_on_error: false,
                ..CompilerOptions::default()
            },
        );
        assert!(output.emitted_files.is_empty(), "{source}: {output:#?}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }

    let checked_source = concat!(
        "const value = async function changed() { };\n",
        "const independent: MissingIndependent = 1;\n",
    );
    let service = language_service(checked_source);
    let semantic = service.semantic_diagnostics("function-expression.ts");
    let [diagnostic] = semantic.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", semantic.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        "const value = async function changed() { };\nconst independent: ".len() as u32
    );
    assert_eq!(diagnostic.length, "MissingIndependent".len() as u32);
    assert_eq!(
        diagnostic.message_text,
        "Cannot find name 'MissingIndependent'."
    );
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
    let checked = service.compile();
    assert_eq!(
        diagnostic_identities(&checked.diagnostics),
        aggregate_product_identities(checked_source, &semantic.diagnostics),
    );
    assert_eq!(checked.semantic_completion, SemanticCompletion::Deferred);

    for source in [
        "const value = function changed() { };",
        "const async = 1;\nconst value = async\nfunction changed() { }\n",
    ] {
        let output = compile(source, false, true);
        assert!(
            output.emitted_files.iter().any(|file| !file.declaration),
            "{source}: {output:#?}",
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}
