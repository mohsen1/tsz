use super::*;

fn diagnostic_identity(output: &tsz::CompileOutput) -> Vec<(u32, u32, u32, String)> {
    output
        .diagnostics
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

#[test]
fn contextual_parameter_type_queries_do_not_reuse_precontext_any() {
    for (name, source, mismatch) in [
        (
            "arrow",
            concat!(
                "const callback: (cedar: number) => void = cedar => {",
                "const wrong: typeof cedar = 'bad'; };",
            ),
            "wrong",
        ),
        (
            "wrapped renamed arrow",
            concat!(
                "const callback: (input: number) => void = ((birch) => {",
                "const rejected: typeof birch = 'bad'; });",
            ),
            "rejected",
        ),
        (
            "function expression",
            concat!(
                "const callback: (input: number) => void = function (maple) {",
                "const denied: typeof maple = 'bad'; };",
            ),
            "denied",
        ),
        (
            "indexed access through contextual parameter",
            concat!(
                "const callback: (input: { x: number }) => void = input => {",
                "const rejected: typeof input[\"x\"] = 'bad'; };",
            ),
            "rejected",
        ),
    ] {
        let expected = vec![(
            2322,
            source.find(mismatch).unwrap() as u32,
            mismatch.len() as u32,
            "Type 'string' is not assignable to type 'number'.".to_string(),
        )];
        let compiler = Compiler::new();
        for iteration in 0..2 {
            let output = compiler.compile(
                vec![SourceInput::new(
                    "function-expression.ts",
                    Arc::<str>::from(source),
                )],
                &CompilerOptions {
                    strict: true,
                    no_emit: true,
                    ..CompilerOptions::default()
                },
            );
            assert_eq!(
                diagnostic_identity(&output),
                expected,
                "{name}, compile {iteration}: {output:#?}",
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "{name}, compile {iteration}: {output:#?}",
            );
            assert_eq!(
                output.exit_status,
                CompileExitStatus::DiagnosticsPresentOutputsSkipped,
                "{name}, compile {iteration}: {output:#?}",
            );
        }

        let service = language_service(source);
        for iteration in 0..2 {
            let semantic = service.semantic_diagnostics("function-expression.ts");
            assert_eq!(
                semantic
                    .diagnostics
                    .iter()
                    .map(|diagnostic| (
                        diagnostic.code,
                        diagnostic.start,
                        diagnostic.length,
                        diagnostic.message_text.clone(),
                    ))
                    .collect::<Vec<_>>(),
                expected,
                "{name}, service query {iteration}: {semantic:#?}",
            );
            assert_eq!(
                semantic.semantic_completion,
                SemanticCompletion::Complete,
                "{name}, service query {iteration}: {semantic:#?}",
            );
        }
    }
}

#[test]
fn contextual_parameter_type_queries_keep_positive_and_fallback_paths_stable() {
    let positive = concat!(
        "const callback: (spruce: number) => void = spruce => {",
        "const kept: typeof spruce = 1; const alsoKept: number = spruce; };",
    );
    let compiler = Compiler::new();
    for iteration in 0..2 {
        let output = compiler.compile(
            vec![SourceInput::new(
                "function-expression.ts",
                Arc::<str>::from(positive),
            )],
            &CompilerOptions {
                strict: true,
                no_emit: true,
                ..CompilerOptions::default()
            },
        );
        assert!(
            output.diagnostics.is_empty(),
            "positive compile {iteration}: {:#?}",
            output.diagnostics,
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "positive compile {iteration}: {output:#?}",
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success);
    }

    // Without a contextual signature, the early provisional `any` remains
    // query-local until the function owner publishes the final `any` type.
    let uncontextualized = concat!(
        "const callback = (fir) => {",
        "const kept: typeof fir = 'anything'; return fir; };",
    );
    for iteration in 0..2 {
        let output = compiler.compile(
            vec![SourceInput::new(
                "function-expression.ts",
                Arc::<str>::from(uncontextualized),
            )],
            &CompilerOptions {
                strict: false,
                no_emit: true,
                ..CompilerOptions::default()
            },
        );
        assert!(
            output.diagnostics.is_empty(),
            "uncontextualized compile {iteration}: {:#?}",
            output.diagnostics,
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "uncontextualized compile {iteration}: {output:#?}",
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success);
    }

    // Generic contextual assignment is still outside the claimed function
    // expression subset. Its parameter query must remain fail-closed and must
    // not gain a fabricated concrete mismatch from a provisional `any`.
    let generic = concat!(
        "const callback: <Cedar>(value: Cedar) => void = <Birch>(renamed: Birch) => {",
        "const pending: typeof renamed = 1; };",
    );
    for iteration in 0..2 {
        let output = compiler.compile(
            vec![SourceInput::new(
                "function-expression.ts",
                Arc::<str>::from(generic),
            )],
            &CompilerOptions {
                strict: true,
                no_emit: true,
                ..CompilerOptions::default()
            },
        );
        assert!(
            output.diagnostics.is_empty(),
            "generic compile {iteration}: {:#?}",
            output.diagnostics,
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "generic compile {iteration}: {output:#?}",
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn indexed_contextual_parameter_queries_are_cold_warm_and_root_order_stable() {
    let source = concat!(
        "const callback: (input: { x: number }) => void = input => {",
        "const rejected: typeof input[\"x\"] = 'bad'; };",
    );
    let sibling = "export const independent = 1;";
    let expected = vec![(
        2322,
        source.find("rejected").unwrap() as u32,
        "rejected".len() as u32,
        "Type 'string' is not assignable to type 'number'.".to_string(),
    )];
    let compiler = Compiler::new();
    for reversed in [false, true] {
        let roots = if reversed {
            vec![
                SourceInput::new("sibling.ts", Arc::<str>::from(sibling)),
                SourceInput::new("function-expression.ts", Arc::<str>::from(source)),
            ]
        } else {
            vec![
                SourceInput::new("function-expression.ts", Arc::<str>::from(source)),
                SourceInput::new("sibling.ts", Arc::<str>::from(sibling)),
            ]
        };
        for iteration in 0..2 {
            let output = compiler.compile(
                roots.clone(),
                &CompilerOptions {
                    strict: true,
                    no_emit: true,
                    ..CompilerOptions::default()
                },
            );
            assert_eq!(
                diagnostic_identity(&output),
                expected,
                "reversed={reversed}, compile={iteration}: {output:#?}",
            );
            assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
            assert_eq!(
                output.exit_status,
                CompileExitStatus::DiagnosticsPresentOutputsSkipped
            );
        }
    }
}
