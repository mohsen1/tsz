use std::sync::Arc;

use tsz::diagnostics::{DiagnosticCategory, RelatedInformation};
use tsz::service::LanguageService;
use tsz::{CompileExitStatus, CompilerOptions, SemanticCompletion};

type DiagnosticFingerprint = (
    String,
    u32,
    u32,
    u32,
    DiagnosticCategory,
    String,
    Vec<(String, u32, u32, u32, String, u32)>,
);

fn related_fingerprint(
    related: &[RelatedInformation],
) -> Vec<(String, u32, u32, u32, String, u32)> {
    related
        .iter()
        .map(|related| {
            (
                related.file.clone(),
                related.code,
                related.start,
                related.length,
                related.message_text.clone(),
                related.depth,
            )
        })
        .collect()
}

fn semantic_fingerprint(
    result: &tsz::service::SemanticDiagnosticResult,
) -> Vec<DiagnosticFingerprint> {
    result
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.file.clone(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.clone(),
                related_fingerprint(&diagnostic.related_information),
            )
        })
        .collect()
}

fn options() -> CompilerOptions {
    CompilerOptions {
        target: "es2015".to_string(),
        strict: true,
        no_emit: true,
        ..CompilerOptions::default()
    }
}

fn missing_name(path: &str, source: &str, name: &str) -> DiagnosticFingerprint {
    (
        path.to_string(),
        2304,
        source.find(name).expect("missing-name witness") as u32,
        name.len() as u32,
        DiagnosticCategory::Error,
        format!("Cannot find name '{name}'."),
        Vec::new(),
    )
}

fn missing_name_at(path: &str, start: u32, name: &str) -> DiagnosticFingerprint {
    (
        path.to_string(),
        2304,
        start,
        name.len() as u32,
        DiagnosticCategory::Error,
        format!("Cannot find name '{name}'."),
        Vec::new(),
    )
}

#[test]
fn recovered_type_tail_defers_only_its_dependency_closed_owner() {
    let cases = [
        (
            "after.ts",
            concat!(
                "type Broken<Payload> = ['head', ...Payload[]] | ['tail', Payload];\n",
                "const kept: MissingAfter = 1;\n",
            ),
            "MissingAfter",
        ),
        (
            "before.ts",
            concat!(
                "const kept: MissingBefore = 1;\n",
                "type Renamed<Item> = ['head', ...Item[]] | ['tail', Item];\n",
            ),
            "MissingBefore",
        ),
        (
            "nested.ts",
            concat!(
                "function wrapper() {\n",
                "  type Nested<Element> = ['head', ...Element[]] | ['tail', Element];\n",
                "  let dependent: Nested<string>;\n",
                "}\n",
                "const kept: MissingOutside = 1;\n",
            ),
            "MissingOutside",
        ),
    ];

    for (path, source, independent) in cases {
        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));

        let result = service.semantic_diagnostics(path);
        assert_eq!(
            result.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
        assert_eq!(
            semantic_fingerprint(&result),
            vec![missing_name(path, source, independent)],
            "{path}: {:#?}",
            result.diagnostics
        );
        assert_eq!(
            service.compile().exit_status,
            CompileExitStatus::SemanticIncomplete,
            "{path}"
        );
    }
}

#[test]
fn recovered_type_producer_defers_cross_file_consumers_in_path_order_variants() {
    let producer = "type Broken<Value> = ['head', ...Value[]] | ['tail', Value];\n";
    let consumer = "let dependent: Broken<string>; const kept: MissingConsumer = 1;\n";
    let safe = "const kept: MissingSafe = 1;\n";

    for (producer_path, consumer_path) in [
        ("a-producer.ts", "z-consumer.ts"),
        ("z-producer.ts", "a-consumer.ts"),
    ] {
        let mut service = LanguageService::new(options());
        service.open(producer_path, Arc::<str>::from(producer));
        service.open(consumer_path, Arc::<str>::from(consumer));
        service.open("m-safe.ts", Arc::<str>::from(safe));

        let producer_result = service.semantic_diagnostics(producer_path);
        assert_eq!(
            producer_result.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert!(
            semantic_fingerprint(&producer_result).is_empty(),
            "{producer_path}: {:#?}",
            producer_result.diagnostics
        );

        let consumer_result = service.semantic_diagnostics(consumer_path);
        assert_eq!(
            consumer_result.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert_eq!(
            semantic_fingerprint(&consumer_result),
            vec![missing_name(consumer_path, consumer, "MissingConsumer")]
        );

        let safe_result = service.semantic_diagnostics("m-safe.ts");
        assert_eq!(
            safe_result.semantic_completion,
            SemanticCompletion::Complete
        );
        assert_eq!(
            semantic_fingerprint(&safe_result),
            vec![missing_name("m-safe.ts", safe, "MissingSafe")]
        );
    }
}

#[test]
fn flat_recovery_declaration_fragments_defer_consumers_but_closed_siblings_remain_owned() {
    let producer = concat!(
        "const tag: any = 0; tag `x` const leaked = 1; ",
        "const closed: number = 1;\n",
    );
    let consumer = concat!(
        "const dependent: string = leaked; ",
        "const kept: MissingFragmentConsumer = 1;\n",
    );
    let safe = concat!(
        "const copy: number = closed; ",
        "const kept: MissingClosedControl = 1;\n",
    );

    for (producer_path, consumer_path) in [
        ("a-fragment.ts", "z-consumer.ts"),
        ("z-fragment.ts", "a-consumer.ts"),
    ] {
        let mut service = LanguageService::new(options());
        service.open(producer_path, Arc::<str>::from(producer));
        service.open(consumer_path, Arc::<str>::from(consumer));
        service.open("m-safe.ts", Arc::<str>::from(safe));

        let consumer_result = service.semantic_diagnostics(consumer_path);
        assert_eq!(
            consumer_result.semantic_completion,
            SemanticCompletion::Deferred,
            "{producer_path} -> {consumer_path}",
        );
        assert_eq!(
            semantic_fingerprint(&consumer_result),
            vec![missing_name(
                consumer_path,
                consumer,
                "MissingFragmentConsumer",
            )],
            "{consumer_path}: {:#?}",
            consumer_result.diagnostics,
        );

        let safe_result = service.semantic_diagnostics("m-safe.ts");
        assert_eq!(
            safe_result.semantic_completion,
            SemanticCompletion::Complete
        );
        assert_eq!(
            semantic_fingerprint(&safe_result),
            vec![missing_name("m-safe.ts", safe, "MissingClosedControl")],
        );
    }
}

#[test]
fn complete_type_syntax_keeps_missing_names_definitive() {
    let source = concat!(
        "type Complete<Value> = ['head', Value[]] | ['tail', Value];\n",
        "type StillMissing = MissingOwned;\n",
    );
    let mut service = LanguageService::new(options());
    service.open("complete.ts", Arc::<str>::from(source));

    let result = service.semantic_diagnostics("complete.ts");
    assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![missing_name("complete.ts", source, "MissingOwned")]
    );
}

#[test]
fn recovered_expression_and_declaration_tails_share_the_scoped_boundary() {
    let cases = [
        (
            "generic-arrow.ts",
            concat!(
                "const identity = <Value>(value: Value) => value;\n",
                "const kept: MissingArrowSibling = 1;\n",
            ),
            "MissingArrowSibling",
        ),
        (
            "definite.ts",
            concat!(
                "let renamed!: symbol;\n",
                "const dependent = renamed;\n",
                "const kept: MissingDefiniteSibling = 1;\n",
            ),
            "MissingDefiniteSibling",
        ),
        (
            "binding-pattern.ts",
            concat!(
                "declare function make(): {first: number; second: string};\n",
                "const { first, second } = make();\n",
                "const kept: MissingPatternSibling = 1;\n",
            ),
            "MissingPatternSibling",
        ),
    ];

    for (path, source, independent) in cases {
        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));

        let result = service.semantic_diagnostics(path);
        assert_eq!(
            result.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
        assert_eq!(
            semantic_fingerprint(&result),
            vec![missing_name(path, source, independent)],
            "{path}: {:#?}",
            result.diagnostics
        );
    }
}

#[test]
fn modeled_postfix_element_access_reports_the_previously_recovered_relation() {
    let cases = [
        (
            "postfix.ts",
            concat!(
                "let receiver: string[] = [];\n",
                "let value = 0;\n",
                "const before: MissingBefore = 1;\n",
                "receiver[1] = (value = 1);\n",
                "const after: MissingAfter = 1;\n",
            ),
            "(value = 1)",
        ),
        (
            "postfix-asi.ts",
            concat!(
                "let renamed: string[] = [];\n",
                "let value = 0;\n",
                "const before: MissingBefore = 1;\n",
                "renamed\n",
                "[2] = (value = 2)\n",
                "const after: MissingAfter = 1;\n",
            ),
            "(value = 2)",
        ),
        (
            "postfix-nested.ts",
            concat!(
                "const before: MissingBefore = 1;\n",
                "function nested() {\n",
                "  let wrapped: string[] = [];\n",
                "  let renamed = 0;\n",
                "  (wrapped)\n",
                "  [(1)] = ((renamed = (2)));\n",
                "  const inside: MissingInside = 1;\n",
                "}\n",
                "const after: MissingAfter = 1;\n",
            ),
            "((renamed = (2)))",
        ),
    ];

    for (path, source, relation) in cases {
        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));

        let result = service.semantic_diagnostics(path);
        assert_eq!(
            result.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
        let mut expected = vec![missing_name(path, source, "MissingBefore")];
        expected.push((
            path.to_string(),
            2322,
            source.find(relation).expect("assignment source") as u32,
            relation.len() as u32,
            DiagnosticCategory::Error,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            Vec::new(),
        ));
        if source.contains("MissingInside") {
            expected.push(missing_name(path, source, "MissingInside"));
        }
        expected.push(missing_name(path, source, "MissingAfter"));
        assert_eq!(
            semantic_fingerprint(&result),
            expected,
            "{path}: {:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn assignment_expression_values_remain_their_sources() {
    let path = "assignment-source.ts";
    let source = concat!(
        "let holder: any;\n",
        "const good: number = (holder = 1);\n",
        "const bad: string = ((holder = 2));\n",
    );
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![(
            path.to_string(),
            2322,
            source.find("bad").expect("bad assignment") as u32,
            "bad".len() as u32,
            DiagnosticCategory::Error,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            Vec::new(),
        )],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn class_template_recovery_uses_the_authored_outer_class_boundary() {
    let cases = [
        (
            "instance-private.ts",
            concat!(
                "class Vessel {\n",
                "  #invoke = function() { this.payload = 1; };\n",
                "  payload = 0;\n",
                "  execute() {\n",
                "    const tagged = this.#invoke`head${1}tail`;\n",
                "    this.getVessel().#invoke`left${2}right`;\n",
                "  }\n",
                "  getVessel() { return new Vessel(); }\n",
                "}\n",
                "const kept: MissingAfterClass = 1;\n",
            ),
        ),
        (
            "static-private.ts",
            concat!(
                "class Renamed {\n",
                "  static #invoke = function() { this.payload = 1; };\n",
                "  static payload = 0;\n",
                "  execute() {\n",
                "    const tagged = Renamed.#invoke`head${1}tail`;\n",
                "    this.getClass().#invoke`left${2}right`;\n",
                "  }\n",
                "  getClass() { return Renamed; }\n",
                "}\n",
                "const kept: MissingAfterClass = 1;\n",
            ),
        ),
    ];

    for (path, source) in cases {
        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));

        let result = service.semantic_diagnostics(path);
        assert_eq!(
            result.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
        assert_eq!(
            semantic_fingerprint(&result),
            vec![missing_name(path, source, "MissingAfterClass")],
            "{path}: {:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn using_recovery_honors_newline_asi_and_preserves_the_next_declaration() {
    let producer = concat!(
        "using recovered = acquire()\n",
        "const preserved: string = 'owned';\n",
    );
    let consumer = concat!(
        "const dependent: string = preserved;\n",
        "const kept: MissingUsingConsumer = 1;\n",
    );
    let mut service = LanguageService::new(options());
    service.open("a-using.ts", Arc::<str>::from(producer));
    service.open("z-consumer.ts", Arc::<str>::from(consumer));

    let producer_result = service.semantic_diagnostics("a-using.ts");
    assert_eq!(
        producer_result.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(
        semantic_fingerprint(&producer_result).is_empty(),
        "{:#?}",
        producer_result.diagnostics,
    );

    let consumer_result = service.semantic_diagnostics("z-consumer.ts");
    assert_eq!(
        consumer_result.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        semantic_fingerprint(&consumer_result),
        vec![missing_name(
            "z-consumer.ts",
            consumer,
            "MissingUsingConsumer",
        )],
        "{:#?}",
        consumer_result.diagnostics,
    );
}

#[test]
fn recovered_binding_patterns_publish_only_authored_binding_identities() {
    let path = "binding-identities.ts";
    let source = concat!(
        "const source: any = {};\n",
        "const { sourceKey: renamed, nested: { value: deep }, list: [head, ...tail], shorthand = 1, ...rest } = source;\n",
        "renamed; deep; head; tail; shorthand; rest;\n",
        "sourceKey;\n",
        "const kept: MissingPatternSibling = 1;\n",
    );
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![
            missing_name_at(
                path,
                source.rfind("sourceKey").expect("property-key reference") as u32,
                "sourceKey",
            ),
            missing_name(path, source, "MissingPatternSibling"),
        ],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn recovered_variable_lists_publish_every_authored_binding_identity() {
    let cases = [
        (
            "direct-list.ts",
            concat!(
                "declare var invoke, left, right;\n",
                "invoke(left, right);\n",
                "const kept: MissingDirectSibling = 1;\n",
            ),
            "MissingDirectSibling",
        ),
        (
            "wrapped-list.ts",
            concat!(
                "declare let dispatch, first, second;\n",
                "function outer() {\n",
                "  function inner() { dispatch(first, second); }\n",
                "  inner();\n",
                "}\n",
                "const kept: MissingWrappedSibling = 1;\n",
            ),
            "MissingWrappedSibling",
        ),
    ];

    for (path, source, independent) in cases {
        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));

        let result = service.semantic_diagnostics(path);
        assert_eq!(
            result.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
        assert_eq!(
            semantic_fingerprint(&result),
            vec![missing_name(path, source, independent)],
            "{path}: {:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn recovered_variable_list_scanner_skips_initializer_commas() {
    let path = "initializer-list.ts";
    let source = concat!(
        "let invoke: any, first = invoke(1, nestedOnly), last = 'owned';\n",
        "function wrapped() { invoke(first, last); }\n",
        "nestedOnly;\n",
        "const kept: MissingInitializerSibling = 1;\n",
    );
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![
            missing_name_at(
                path,
                source.find("nestedOnly").expect("initializer reference") as u32,
                "nestedOnly",
            ),
            missing_name_at(
                path,
                source.rfind("nestedOnly").expect("independent reference") as u32,
                "nestedOnly",
            ),
            missing_name(path, source, "MissingInitializerSibling"),
        ],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn recovered_variable_list_producers_defer_cross_file_consumers_in_path_order_variants() {
    let producer = "let renamed: any, payload = [1, 2];\n";
    let consumer = "renamed(payload); const kept: MissingListConsumer = 1;\n";
    let safe = "const kept: MissingListSafe = 1;\n";

    for (producer_path, consumer_path) in [
        ("a-list-producer.ts", "z-list-consumer.ts"),
        ("z-list-producer.ts", "a-list-consumer.ts"),
    ] {
        let mut service = LanguageService::new(options());
        service.open(producer_path, Arc::<str>::from(producer));
        service.open(consumer_path, Arc::<str>::from(consumer));
        service.open("m-list-safe.ts", Arc::<str>::from(safe));

        let producer_result = service.semantic_diagnostics(producer_path);
        assert_eq!(
            producer_result.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert!(
            semantic_fingerprint(&producer_result).is_empty(),
            "{producer_path}: {:#?}",
            producer_result.diagnostics,
        );

        let consumer_result = service.semantic_diagnostics(consumer_path);
        assert_eq!(
            consumer_result.semantic_completion,
            SemanticCompletion::Deferred
        );
        assert_eq!(
            semantic_fingerprint(&consumer_result),
            vec![missing_name(consumer_path, consumer, "MissingListConsumer",)],
            "{consumer_path}: {:#?}",
            consumer_result.diagnostics,
        );

        let safe_result = service.semantic_diagnostics("m-list-safe.ts");
        assert_eq!(
            safe_result.semantic_completion,
            SemanticCompletion::Complete
        );
        assert_eq!(
            semantic_fingerprint(&safe_result),
            vec![missing_name("m-list-safe.ts", safe, "MissingListSafe")],
            "{:#?}",
            safe_result.diagnostics,
        );
    }
}

#[test]
fn ordinary_variable_declaration_keeps_a_real_missing_name_definitive() {
    let path = "ordinary-variable.ts";
    let source = "declare var owned; owned; missingReal;\n";
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![missing_name(path, source, "missingReal")],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn eof_recovery_does_not_retroactively_claim_a_prior_statement() {
    let path = "eof-recovery.ts";
    let source = concat!(
        "const kept: MissingBeforeEof = 1;\n",
        "type Broken = `unterminated${string\n",
    );
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![missing_name(path, source, "MissingBeforeEof")],
        "{:#?}",
        result.diagnostics,
    );
}
