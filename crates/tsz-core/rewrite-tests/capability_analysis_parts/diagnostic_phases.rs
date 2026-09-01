use super::*;

#[test]
fn syntax_phase_selection_keeps_cross_file_semantic_products_independent() {
    let producer = concat!(
        "let subject: string | number = 0;\n",
        "switch (subject.) { default: break; }\n",
        "const produced = subject;\n",
    );
    let consumer = concat!(
        "declare function acceptsString(value: string): void;\n",
        "const composed = produced + 1;\n",
        "const alias = composed;\n",
        "const dependent: string = alias;\n",
        "acceptsString(composed);\n",
        "const member = (produced + 1).toFixed;\n",
        "const subtracted = produced - 1;\n",
        "const multiplied = produced * 1;\n",
        "const divided = produced / 1;\n",
        "const remainder = produced % 1;\n",
        "const subtractDependent: string = subtracted;\n",
        "const multiplyDependent: string = multiplied;\n",
        "const divideDependent: string = divided;\n",
        "const remainderDependent: string = remainder;\n",
        "let assignmentTarget: string | number = 0;\n",
        "const assignmentValue = (assignmentTarget = produced + 1);\n",
        "const assignmentDependent: string = assignmentValue;\n",
        "const concrete = 4 - 2;\n",
        "const concreteWrong: string = concrete;\n",
        "const stableLeft = \"\" + produced;\n",
        "const stableLeftWrong: number = stableLeft;\n",
        "const stableRight = produced + \"\";\n",
        "const stableRightWrong: number = stableRight;\n",
        "const independent: string = 1;\n",
        "type Kept = MissingConsumerSibling;\n",
    );
    let compiler = Compiler::new();
    let roots = vec![
        SourceInput::new("producer.ts", Arc::<str>::from(producer)),
        SourceInput::new("consumer.ts", Arc::<str>::from(consumer)),
    ];
    let mut reversed = roots.clone();
    reversed.reverse();
    let forward = compiler.compile(roots.clone(), &semantic_options());
    let repeated = compiler.compile(roots, &semantic_options());
    let reverse = compiler.compile(reversed, &semantic_options());
    let expected_semantic = vec![
        (
            "consumer.ts".to_string(),
            2322,
            consumer
                .find("concreteWrong")
                .expect("complete arithmetic relation") as u32,
            "concreteWrong".len() as u32,
            DiagnosticCategory::Error,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            Vec::new(),
        ),
        (
            "consumer.ts".to_string(),
            2322,
            consumer
                .find("stableLeftWrong")
                .expect("complete left string concatenation") as u32,
            "stableLeftWrong".len() as u32,
            DiagnosticCategory::Error,
            "Type 'string' is not assignable to type 'number'.".to_string(),
            Vec::new(),
        ),
        (
            "consumer.ts".to_string(),
            2322,
            consumer
                .find("stableRightWrong")
                .expect("complete right string concatenation") as u32,
            "stableRightWrong".len() as u32,
            DiagnosticCategory::Error,
            "Type 'string' is not assignable to type 'number'.".to_string(),
            Vec::new(),
        ),
        (
            "consumer.ts".to_string(),
            2322,
            consumer.find("independent").expect("independent relation") as u32,
            "independent".len() as u32,
            DiagnosticCategory::Error,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            Vec::new(),
        ),
        (
            "consumer.ts".to_string(),
            2304,
            consumer
                .find("MissingConsumerSibling")
                .expect("independent required type") as u32,
            "MissingConsumerSibling".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingConsumerSibling'.".to_string(),
            Vec::new(),
        ),
    ];
    let expected_syntactic = vec![
        (
            "producer.ts".to_string(),
            1003,
            producer.find(".)").expect("recovered member") as u32 + 1,
            1,
            DiagnosticCategory::Error,
            "Identifier expected.".to_string(),
            Vec::new(),
        ),
        (
            "producer.ts".to_string(),
            1005,
            producer.find(") {").expect("recovered switch close") as u32 + 2,
            1,
            DiagnosticCategory::Error,
            "')' expected.".to_string(),
            Vec::new(),
        ),
    ];
    let mut service = LanguageService::new(semantic_options());
    service.open("producer.ts", Arc::<str>::from(producer));
    service.open("consumer.ts", Arc::<str>::from(consumer));
    let producer_syntactic = service.syntactic_diagnostics("producer.ts");
    assert_eq!(
        producer_syntactic.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        diagnostics_fingerprint(&producer_syntactic.diagnostics),
        expected_syntactic
    );
    let consumer_syntactic = service.syntactic_diagnostics("consumer.ts");
    assert_eq!(
        consumer_syntactic.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert!(consumer_syntactic.diagnostics.is_empty());
    let producer_semantic = service.semantic_diagnostics("producer.ts");
    assert_eq!(
        producer_semantic.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(semantic_fingerprint(&producer_semantic), []);
    let consumer_semantic = service.semantic_diagnostics("consumer.ts");
    assert_eq!(
        consumer_semantic.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(semantic_fingerprint(&consumer_semantic), expected_semantic);

    let mut reverse_service = LanguageService::new(semantic_options());
    reverse_service.open("consumer.ts", Arc::<str>::from(consumer));
    reverse_service.open("producer.ts", Arc::<str>::from(producer));
    assert_eq!(
        reverse_service
            .syntactic_diagnostics("producer.ts")
            .diagnostics,
        producer_syntactic.diagnostics
    );
    assert_eq!(
        semantic_fingerprint(&reverse_service.semantic_diagnostics("consumer.ts")),
        expected_semantic
    );

    for output in [&forward, &repeated, &reverse] {
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert_eq!(diagnostic_fingerprint(output), expected_syntactic);
    }
    assert_eq!(forward.stats.types, repeated.stats.types);
    assert_eq!(forward.stats.types, reverse.stats.types);
}

#[test]
fn compiler_selects_syntax_before_program_and_semantic_phase_products() {
    let source = concat!(
        "let subject: string | number = 0;\n",
        "switch (subject.) { default: break; }\n",
        "const independent: string = 1;\n",
        "type Kept = MissingSameFileSibling;\n",
    );
    let roots = vec![SourceInput::new(
        "mixed-phases.ts",
        Arc::<str>::from(source),
    )];
    let options = CompilerOptions {
        strict_null_checks: Some(false),
        strict_property_initialization: Some(true),
        no_emit: true,
        declaration: true,
        ..CompilerOptions::default()
    };
    let compiler = Compiler::new();
    let first = compiler.compile(roots.clone(), &options);
    let repeated = compiler.compile(roots, &options);
    let expected = vec![
        (
            "mixed-phases.ts".to_string(),
            1003,
            source.find(".)").expect("recovered member") as u32 + 1,
            1,
            DiagnosticCategory::Error,
            "Identifier expected.".to_string(),
            Vec::new(),
        ),
        (
            "mixed-phases.ts".to_string(),
            1005,
            source.find(") {").expect("recovered switch close") as u32 + 2,
            1,
            DiagnosticCategory::Error,
            "')' expected.".to_string(),
            Vec::new(),
        ),
    ];

    for output in [&first, &repeated] {
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert_eq!(diagnostic_fingerprint(output), expected);
    }

    let unchecked = compiler.compile(
        vec![SourceInput::new(
            "mixed-phases.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            no_check: true,
            ..options
        },
    );
    assert_eq!(unchecked.stats.types, 0);
    assert_eq!(
        diagnostic_fingerprint(&unchecked),
        expected,
        "noCheck preserves the same first nonempty compiler phase without semantic work"
    );
}

#[test]
fn recovered_source_keeps_quick_info_claims_local_without_empty_summaries() {
    let source = concat!(
        "function recovered(value: ) {}\n",
        "const stable: string = 'owned';\n",
    );
    let path = "recovered-quick-info.ts";
    let recovered = source.find("recovered").expect("recovered host") as u32;
    let stable = source.find("stable").expect("independent sibling") as u32;
    let mut service = LanguageService::new(semantic_options());
    service.open(path, Arc::<str>::from(source));

    assert!(matches!(
        service.quick_info(path, recovered),
        tsz::service::ServiceQuery::Nonclaimed(_)
    ));
    let quick_info = service
        .quick_info(path, stable)
        .expect_claimed("independent sibling quick info")
        .expect("independent sibling quick info result");
    assert_eq!(quick_info.kind, "const");
    assert_eq!(quick_info.text_span.start, stable);
    assert_eq!(quick_info.display, "const stable: string");
}
