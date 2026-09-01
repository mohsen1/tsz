use std::sync::Arc;

use tsz::diagnostics::{DiagnosticCategory, RelatedInformation};
use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    ClassMemberKind, ExpressionKind, Parameter, ParameterModifier, StatementKind, TypeNodeKind,
    parse_source,
};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

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

fn parser_codes(source: &str) -> Vec<u32> {
    let source = SourceText::new(FileId(0), "parser-recovery.ts".into(), Arc::from(source));
    parse_source(&source)
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn compile_source(path: &str, source: &str, strict: bool) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(path, Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict,
            no_implicit_any: (!strict).then_some(false),
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    )
}

fn assert_property_parameter_is_recovery_free(parameter: &Parameter, modifier: ParameterModifier) {
    assert_eq!(
        parameter
            .modifiers
            .iter()
            .map(|node| node.kind)
            .collect::<Vec<_>>(),
        [modifier],
    );
    assert!(parameter.overload_completion_supported);
    assert!(parameter.function_implementation_completion_supported);
}

#[test]
fn property_parameter_modifiers_keep_parser_completion_at_the_grammar_owner() {
    let source = concat!(
        "function Direct(public renamed) {}\n",
        "function wrapper() { function Nested(protected changed) {} }\n",
        "type Callable = (private typed) => void;\n",
        "const arrow = (readonly arrowed) => {};\n",
        "function Group(override overloadName): void;\n",
        "function Group(override implementationName) {}\n",
    );
    let source_text = SourceText::new(FileId(0), "parameter-owners.ts".into(), Arc::from(source));
    let parsed = parse_source(&source_text);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

    let StatementKind::Function(direct) = &parsed.unit.statements[0].kind else {
        panic!("direct function declaration");
    };
    assert!(direct.overload_completion_supported);
    assert_property_parameter_is_recovery_free(&direct.parameters[0], ParameterModifier::Public);

    let StatementKind::Function(wrapper) = &parsed.unit.statements[1].kind else {
        panic!("wrapper function declaration");
    };
    let StatementKind::Function(nested) = &wrapper.body[0].kind else {
        panic!("nested function declaration");
    };
    assert!(nested.overload_completion_supported);
    assert_property_parameter_is_recovery_free(&nested.parameters[0], ParameterModifier::Protected);

    let StatementKind::TypeAlias(callable) = &parsed.unit.statements[2].kind else {
        panic!("callable type alias");
    };
    let TypeNodeKind::Function {
        parameters,
        parameter_list_recovered,
        ..
    } = &callable.ty.kind
    else {
        panic!("function type");
    };
    assert!(!parameter_list_recovered);
    assert_property_parameter_is_recovery_free(&parameters[0], ParameterModifier::Private);

    let StatementKind::Variable(variable) = &parsed.unit.statements[3].kind else {
        panic!("arrow variable");
    };
    let ExpressionKind::FunctionLike(arrow) = &variable.declarators[0]
        .initializer
        .as_ref()
        .expect("arrow initializer")
        .kind
    else {
        panic!("arrow function");
    };
    assert_property_parameter_is_recovery_free(&arrow.parameters[0], ParameterModifier::Readonly);

    for statement in &parsed.unit.statements[4..=5] {
        let StatementKind::Function(grouped) = &statement.kind else {
            panic!("function overload group");
        };
        assert!(grouped.overload_completion_supported);
        assert_property_parameter_is_recovery_free(
            &grouped.parameters[0],
            ParameterModifier::Override,
        );
    }
}

#[test]
fn illegal_parameter_properties_report_complete_host_grammar_and_strict_implicit_any() {
    let source = concat!(
        "function Direct(public renamed) {}\n",
        "function wrapper() { function Nested(protected changed) {} }\n",
        "type Callable = (private typed) => void;\n",
        "const arrow = (readonly arrowed) => {};\n",
        "function Group(override overloadName): void;\n",
        "function Group(override implementationName) {}\n",
    );
    let properties = [
        ("public renamed", "renamed"),
        ("protected changed", "changed"),
        ("private typed", "typed"),
        ("readonly arrowed", "arrowed"),
        ("override overloadName", "overloadName"),
        ("override implementationName", "implementationName"),
    ];

    let strict = compile_source("parameter-hosts.ts", source, true);
    let expected = properties
        .iter()
        .flat_map(|(parameter, name)| {
            let start = source.find(parameter).expect("parameter span") as u32;
            let length = parameter.len() as u32;
            [
                (
                    start,
                    length,
                    2369,
                    "A parameter property is only allowed in a constructor implementation."
                        .to_string(),
                ),
                (
                    start,
                    length,
                    7006,
                    format!("Parameter '{name}' implicitly has an 'any' type."),
                ),
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(
        strict
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.start,
                diagnostic.length,
                diagnostic.code,
                diagnostic.message_text.clone(),
            ))
            .collect::<Vec<_>>(),
        expected,
        "{:#?}",
        strict.diagnostics,
    );
    assert_eq!(strict.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        strict.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );

    let loose = compile_source("parameter-hosts.ts", source, false);
    assert_eq!(
        loose
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.start, diagnostic.length, diagnostic.code))
            .collect::<Vec<_>>(),
        properties
            .iter()
            .map(|(parameter, _)| {
                (
                    source.find(parameter).expect("parameter span") as u32,
                    parameter.len() as u32,
                    2369,
                )
            })
            .collect::<Vec<_>>(),
        "{:#?}",
        loose.diagnostics,
    );
    assert_eq!(loose.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn legal_constructor_parameter_properties_defer_at_the_synthesized_owner() {
    let source = concat!(
        "class Legal { constructor(public kept: number) {} }\n",
        "class LegalRenamed { constructor(private renamed: string) {} }\n",
    );
    let source_text = SourceText::new(FileId(0), "legal-properties.ts".into(), Arc::from(source));
    let parsed = parse_source(&source_text);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    for (statement, modifier) in parsed
        .unit
        .statements
        .iter()
        .zip([ParameterModifier::Public, ParameterModifier::Private])
    {
        let StatementKind::Class(class) = &statement.kind else {
            panic!("class declaration");
        };
        let ClassMemberKind::Constructor { parameters, .. } = &class.members[0].kind else {
            panic!("constructor member");
        };
        assert_property_parameter_is_recovery_free(&parameters[0], modifier);
    }

    let output = compile_source("legal-properties.ts", source, true);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn invalid_character_parameter_sibling_remains_a_syntax_product() {
    let source = "function recovered(a,¬) {}";
    for strict in [false, true] {
        let output = compile_source("invalid-parameter.ts", source, strict);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.code,
                    diagnostic.message_text.as_str(),
                ))
                .collect::<Vec<_>>(),
            [(
                source.find('¬').expect("invalid character") as u32,
                1,
                1127,
                "Invalid character.",
            )],
            "strict={strict}: {:#?}",
            output.diagnostics,
        );
    }
}

#[test]
fn invalid_character_statements_do_not_become_missing_names() {
    for (path, source) in [
        ("invalid-character.ts", "\\"),
        ("commented-invalid-character.ts", "\\ /* kept */ ;"),
        ("invalid-after-regexp.ts", "/regexp/ \\ ;"),
    ] {
        assert_eq!(parser_codes(source), vec![1127], "{path}");

        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));
        let result = service.semantic_diagnostics(path);
        assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            Vec::<u32>::new(),
            "{path}: {:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn nested_invalid_character_forms_remain_parser_owned_controls() {
    for source in [
        r"\ /regexp/;",
        r"foo(a, \",
        r"foo(a \",
        r"var v: X<T \",
        r"var arg\u003",
        r"var arg\uxxxx",
        r"a\u",
        r"var \u0031a",
    ] {
        assert!(parser_codes(source).contains(&1127), "{source}");
    }

    for source in [r"var a\u0031 = 1;", r"var \u0061 = 1;"] {
        assert!(parser_codes(source).is_empty(), "{source}");
    }
}

#[test]
fn missing_arrow_types_preserve_structural_delimiters() {
    for (source, marker) in [
        ("const first = (renamed: ) => renamed;", ")"),
        ("const second = (): => 1;", "=>"),
        ("const typed = (renamed: string): => renamed;", "=>"),
        ("const optional = (renamed?): => renamed;", "=>"),
        ("const rest = (...renamed): => renamed;", "=>"),
        (
            "const modified = (public renamed: string): => renamed;",
            "=>",
        ),
        ("const recovered = (renamed: string): @ => renamed;", "@"),
        (
            "declare function third(renamed:, sibling: string): void;",
            ",",
        ),
        ("declare function fourth(renamed: = 1): void;", "="),
        ("interface Fifth { [renamed: ]: string }", "]"),
    ] {
        let source_text =
            SourceText::new(FileId(0), "missing-arrow-type.ts".into(), Arc::from(source));
        let parsed = parse_source(&source_text);
        let [diagnostic] = parsed.diagnostics.as_slice() else {
            panic!(
                "unexpected diagnostics for {source}: {:#?}",
                parsed.diagnostics
            );
        };
        assert_eq!(diagnostic.code, 1110);
        assert_eq!(diagnostic.start, source.find(marker).unwrap() as u32);
        assert_eq!(diagnostic.length, marker.len() as u32);
    }
}

#[test]
fn analysis_only_object_member_recovery_does_not_control_nested_postfix_parse() {
    for operand in ["cnd[1]", "(renamed)[2]", "renamed\n[3]"] {
        let source = format!(
            "function f(cnd: any, renamed: any) {{ return {{ ...({operand} && {{ value: 1 }}), }}; }}"
        );
        assert_eq!(
            parser_codes(&source),
            vec![1003, 1005, 1109, 1109],
            "{source}",
        );
    }
}

#[test]
fn interpolated_template_recovery_consumes_the_owned_template_without_parser_fallout() {
    let path = "interpolated-template.ts";
    for source in [
        "`${renamed}`;",
        "tagged`${renamed}`;",
        "await `${renamed}`;",
    ] {
        assert!(parser_codes(source).is_empty(), "{source}");
    }
    let source = concat!(
        "const callback = ([renamed]) => `${renamed}=${String(renamed)}`;\n",
        "const outside: MissingOutside = 1;\n",
    );
    assert!(parser_codes(source).is_empty());

    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));
    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![missing_name(path, source, "MissingOutside")],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn object_member_recovery_owns_a_postfix_assertion_but_not_the_next_statement() {
    let path = "object-assertion.ts";
    let source = concat!(
        "function changed<T extends { a: string }>(obj: T): T { ",
        "let { a, ...rest } = obj; return { a: 'hello', ...rest } as T; }\n",
        "const outside: MissingOutside = 1;\n",
    );
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));
    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![missing_name(path, source, "MissingOutside")],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn recovered_class_member_lists_do_not_publish_overload_adjacency() {
    let path = "recovered-class-members.ts";
    let source = concat!(
        "class Changed<Key, Value> { ",
        "constructor(values: Iterable<[key: Key, value: Value]> | null = null) { ",
        "for (const { 0: renamedKey, 1: renamedValue } of values) { ",
        "this.renamed(renamedKey, renamedValue); } } ",
        "renamed(key: Key, value: Value): this { return this; } }\n",
        "const outside: MissingOutside = 1;\n",
    );
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));
    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2391),
        "{:#?}",
        result.diagnostics,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.start == source.find("MissingOutside").unwrap() as u32),
        "{:#?}",
        result.diagnostics,
    );

    let stable_path = "stable-member-after-body-recovery.ts";
    let stable_source = "class Stable { body() { const broken = ; } renamed(): void; }";
    let mut service = LanguageService::new(options());
    service.open(stable_path, Arc::<str>::from(stable_source));
    let result = service.semantic_diagnostics(stable_path);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == 2391
                && diagnostic.start == stable_source.find("renamed").unwrap() as u32),
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn modified_binding_pattern_recovery_keeps_the_prior_parser_frontier() {
    let source = "class Changed { scan(const { source: renamed } of values) {} *after() {} }";
    assert_eq!(parser_codes(source), vec![1359, 1005, 1005, 1005]);
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

fn implicit_any_variable(path: &str, source: &str, name: &str) -> DiagnosticFingerprint {
    (
        path.to_string(),
        7005,
        source.find(name).expect("ambient variable declaration") as u32,
        name.len() as u32,
        DiagnosticCategory::Error,
        format!("Variable '{name}' implicitly has an 'any' type."),
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
fn empty_binding_pattern_heads_preserve_function_expression_body_ownership() {
    for (declaration_kind, pattern) in [
        ("var", "{}"),
        ("let", "{}"),
        ("const", "{}"),
        ("var", "[]"),
        ("let", "[]"),
        ("const", "[]"),
    ] {
        let path = format!("empty-{declaration_kind}-binding.ts");
        let source = format!(
            "const source: any = {{}}; const callback = function () {{ {declaration_kind} {pattern} = source; MissingBodySibling; }}; MissingOutside;"
        );
        assert!(parser_codes(&source).is_empty(), "{source}");

        let mut service = LanguageService::new(options());
        service.open(&path, Arc::<str>::from(source.as_str()));
        let result = service.semantic_diagnostics(&path);
        assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(
            semantic_fingerprint(&result),
            vec![
                missing_name(&path, &source, "MissingBodySibling"),
                missing_name(&path, &source, "MissingOutside"),
            ],
            "{path}: {:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn balanced_binding_pattern_heads_preserve_nested_renamed_bindings_and_siblings() {
    for (path, pattern, bindings) in [
        (
            "nested-object-binding.ts",
            "{ outer: { value: renamed }, list: [head, ...tail] }",
            "renamed; head; tail;",
        ),
        (
            "nested-array-binding.ts",
            "[renamed, { inner: deep }, ...rest]",
            "renamed; deep; rest;",
        ),
    ] {
        let source = format!(
            "const source: any = {{}}; const callback = function () {{ let {pattern} = source; {bindings} MissingBodySibling; }}; MissingOutside;"
        );
        assert!(parser_codes(&source).is_empty(), "{source}");

        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source.as_str()));
        let result = service.semantic_diagnostics(path);
        assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(
            semantic_fingerprint(&result),
            vec![
                missing_name(path, &source, "MissingBodySibling"),
                missing_name(path, &source, "MissingOutside"),
            ],
            "{path}: {:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn malformed_binding_pattern_heads_keep_identifier_recovery() {
    for source in [
        "const callback = function () { let { = value; };",
        "const callback = function () { let [ = value; };",
        "const callback = function () { let { : value; };",
        "const callback = function () { let [ : value; };",
    ] {
        assert_eq!(parser_codes(source).first(), Some(&1003), "{source}");
    }
}

#[test]
fn recovered_parameter_binding_heads_preserve_default_and_body_ownership() {
    let path = "parameter-binding-defaults.ts";
    let source = concat!(
        "const source: any = {}; const callback = function () { ",
        "function nested({ value: renamed, ...rest } = source, [head] = source) { ",
        "renamed; rest; head; MissingFunctionBody; } ",
        "MissingIifeSibling; }; MissingOutside;",
    );
    assert!(parser_codes(source).is_empty(), "{source}");

    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));
    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![
            missing_name(path, source, "MissingFunctionBody"),
            missing_name(path, source, "MissingIifeSibling"),
            missing_name(path, source, "MissingOutside"),
        ],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn recovered_parameter_default_stays_dependency_closed() {
    let path = "missing-parameter-default.ts";
    let source = concat!(
        "function nested({} = MissingDefault) { MissingFunctionBody; } ",
        "MissingOutside;",
    );
    assert!(parser_codes(source).is_empty(), "{source}");

    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));
    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![
            missing_name(path, source, "MissingFunctionBody"),
            missing_name(path, source, "MissingOutside"),
        ],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn malformed_parameter_binding_heads_keep_identifier_recovery() {
    for source in [
        "const callback = function () { function nested({ = source) {} };",
        "const callback = function () { function nested([ = source) {} };",
        "const callback = function ({...",
        "const callback = function ([...",
    ] {
        assert_eq!(parser_codes(source).first(), Some(&1003), "{source}");
    }
}

#[test]
fn ambient_variable_lists_report_each_authored_name_through_wrapped_uses() {
    let cases = [
        (
            "direct-list.ts",
            concat!(
                "declare var invoke, left, right;\n",
                "invoke(left, right);\n",
                "const kept: MissingDirectSibling = 1;\n",
            ),
            ["invoke", "left", "right"],
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
            ["dispatch", "first", "second"],
            "MissingWrappedSibling",
        ),
    ];

    for (path, source, ambient_names, independent) in cases {
        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));

        let result = service.semantic_diagnostics(path);
        assert_eq!(
            result.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
        assert_eq!(
            semantic_fingerprint(&result),
            vec![
                implicit_any_variable(path, source, ambient_names[0]),
                implicit_any_variable(path, source, ambient_names[1]),
                implicit_any_variable(path, source, ambient_names[2]),
                missing_name(path, source, independent),
            ],
            "{path}: {:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn variable_list_scanner_skips_initializer_commas_with_complete_diagnostics() {
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
    assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
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
fn recovery_fragments_defer_while_concrete_object_methods_keep_identifier_diagnostics() {
    let cases = [
        (
            "loop-fragment.ts",
            concat!(
                "const callback = function () { for (let loopKey in {}) {} ",
                "MissingBodySibling; };\nMissingOutside;\n",
            ),
            vec!["MissingOutside"],
            SemanticCompletion::Deferred,
        ),
        (
            "destructuring-fragment.ts",
            concat!(
                "const callback = function () { let {} = missingPattern; ",
                "MissingBodySibling; };\nMissingOutside;\n",
            ),
            vec!["MissingBodySibling", "MissingOutside"],
            SemanticCompletion::Deferred,
        ),
        (
            "satisfies-fragment.ts",
            concat!(
                "const callback = function () { MissingOwnedBody; } satisfies MissingTail;\n",
                "MissingOutside;\n",
            ),
            vec!["MissingOwnedBody", "MissingOutside"],
            SemanticCompletion::Deferred,
        ),
        (
            "object-member-fragment.ts",
            concat!(
                "const value = { kept: function () { MissingOwnedBody; }, method() { MissingTail; } };\n",
                "MissingOutside;\n",
            ),
            vec!["MissingOwnedBody", "MissingTail", "MissingOutside"],
            SemanticCompletion::Complete,
        ),
        (
            "object-spread-fragment.ts",
            concat!(
                "const value = { ...(function () { MissingOwnedBody; }) };\n",
                "MissingOutside;\n",
            ),
            vec!["MissingOwnedBody", "MissingOutside"],
            SemanticCompletion::Deferred,
        ),
        (
            "arrow-spread-fragment.ts",
            concat!(
                "const value = { ...(() => { MissingOwnedBody; }) };\n",
                "MissingOutside;\n",
            ),
            vec!["MissingOwnedBody", "MissingOutside"],
            SemanticCompletion::Deferred,
        ),
    ];

    for (path, source, expected, completion) in cases {
        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));
        let result = service.semantic_diagnostics(path);
        assert_eq!(result.semantic_completion, completion, "{path}");
        assert_eq!(
            semantic_fingerprint(&result),
            expected
                .into_iter()
                .map(|name| missing_name(path, source, name))
                .collect::<Vec<_>>(),
            "{path}: {:#?}",
            result.diagnostics,
        );
        if path == "loop-fragment.ts" {
            // TypeScript diagnoses the same-body sibling; it becomes independent
            // when the parser owns `for` rather than recovering through this suffix.
            for name in ["loopKey", "MissingBodySibling"] {
                let start = source.find(name).expect("recovery-path witness") as u32;
                assert!(
                    result
                        .diagnostics
                        .iter()
                        .all(|diagnostic| diagnostic.start != start)
                );
            }
        }
    }
}

#[test]
fn contextual_accessor_names_and_signature_this_parse_without_recovery() {
    assert!(
        parser_codes(concat!(
            "class C { set: boolean; get = 1; set(x) {} get() {} ",
            "get value() { return 1; } set value(x) {} }",
        ))
        .is_empty(),
    );
    assert!(
        parser_codes(concat!(
            "type Callback<This, Args extends any[], Return> = ",
            "(this: This, ...args: Args) => Return; ",
            "type Generic = <Value>(this: Value, value: Value) => Value;",
        ))
        .is_empty(),
    );
}

#[test]
fn explicit_this_call_signature_does_not_count_as_a_runtime_parameter() {
    let path = "explicit-this-call-signature.ts";
    let source = concat!(
        "declare const callback: { ",
        "(this: { tag: string }, value: number): void }; callback(1);",
    );
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));
    let result = service.semantic_diagnostics(path);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn variable_list_producers_complete_cross_file_consumers_in_path_order_variants() {
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
            SemanticCompletion::Complete
        );
        assert!(
            semantic_fingerprint(&producer_result).is_empty(),
            "{producer_path}: {:#?}",
            producer_result.diagnostics,
        );

        let consumer_result = service.semantic_diagnostics(consumer_path);
        assert_eq!(
            consumer_result.semantic_completion,
            SemanticCompletion::Complete
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
fn ordinary_ambient_variable_reports_implicit_any_and_keeps_missing_name_definitive() {
    let path = "ordinary-variable.ts";
    let source = "declare var owned; owned; missingReal;\n";
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![
            implicit_any_variable(path, source, "owned"),
            missing_name(path, source, "missingReal"),
        ],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn ambient_variable_implicit_any_respects_annotations_and_effective_options() {
    let annotated_path = "annotated-ambient-list.ts";
    let annotated_source = concat!(
        "declare var alpha: string, beta: number;\n",
        "function wrapped() { alpha; beta; }\n",
        "const kept: MissingAnnotatedSibling = 1;\n",
    );
    let mut annotated_service = LanguageService::new(options());
    annotated_service.open(annotated_path, Arc::<str>::from(annotated_source));
    let annotated_result = annotated_service.semantic_diagnostics(annotated_path);
    assert_eq!(
        annotated_result.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        semantic_fingerprint(&annotated_result),
        vec![missing_name(
            annotated_path,
            annotated_source,
            "MissingAnnotatedSibling",
        )],
        "{:#?}",
        annotated_result.diagnostics,
    );

    let opted_out_path = "ambient-list-no-implicit-any-false.ts";
    let opted_out_source = concat!(
        "declare let looseFirst, looseSecond;\n",
        "function wrapped() { looseFirst; looseSecond; }\n",
        "const kept: MissingOptOutSibling = 1;\n",
    );
    let opted_out_options = CompilerOptions {
        no_implicit_any: Some(false),
        ..options()
    };
    let mut opted_out_service = LanguageService::new(opted_out_options);
    opted_out_service.open(opted_out_path, Arc::<str>::from(opted_out_source));
    let opted_out_result = opted_out_service.semantic_diagnostics(opted_out_path);
    assert_eq!(
        opted_out_result.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        semantic_fingerprint(&opted_out_result),
        vec![missing_name(
            opted_out_path,
            opted_out_source,
            "MissingOptOutSibling",
        )],
        "{:#?}",
        opted_out_result.diagnostics,
    );

    let no_check_path = "ambient-list-no-check.ts";
    let no_check_source = concat!(
        "declare var skippedFirst, skippedSecond;\n",
        "skippedFirst; skippedSecond; MissingNoCheckSibling;\n",
    );
    let no_check_options = CompilerOptions {
        no_check: true,
        ..options()
    };
    let mut no_check_service = LanguageService::new(no_check_options);
    no_check_service.open(no_check_path, Arc::<str>::from(no_check_source));
    let no_check_result = no_check_service.semantic_diagnostics(no_check_path);
    assert_eq!(
        no_check_result.semantic_completion,
        SemanticCompletion::Complete
    );
    assert!(
        semantic_fingerprint(&no_check_result).is_empty(),
        "{:#?}",
        no_check_result.diagnostics,
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

#[test]
fn opaque_member_and_loop_syntax_keep_their_recovery_local() {
    let source = concat!(
        "class Computed { *[renamed]() { MissingGeneratorBody; } }\n",
        "const object = { get renamed() { MissingAccessorBody; return 1; } };\n",
        "for (const { value: renamed } of values) { renamed; MissingLoopBody; }\n",
        "const independent: MissingIndependent = 1;\n",
    );
    assert!(parser_codes(source).is_empty());

    let path = "opaque-syntax-owners.ts";
    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));
    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![
            missing_name(path, source, "MissingLoopBody"),
            missing_name(path, source, "MissingIndependent"),
        ],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn jsx_recovery_fragments_defer_without_hiding_adjacent_missing_names() {
    let declarations = concat!(
        "declare namespace JSX {\n",
        "  interface Element {}\n",
        "  interface IntrinsicElements { [name: string]: any }\n",
        "}\n",
        "declare var React: any;\n",
    );
    let cases = [
        (
            "direct-jsx-recovery.tsx",
            format!(
                "{declarations}<section>Be cautious of &quot;-tail!</section>;\nconst kept: MissingTsxSibling = 1;\n"
            ),
            "MissingTsxSibling",
        ),
        (
            "wrapped-jsx-recovery.tsx",
            format!(
                "{declarations}function wrapped() {{\n  <article>Nested renamed text &amp; tail</article>;\n  const kept: MissingNestedSibling = 1;\n}}\n"
            ),
            "MissingNestedSibling",
        ),
    ];
    for (path, source, missing) in &cases {
        let mut service = LanguageService::new(options());
        service.open(*path, Arc::<str>::from(source.clone()));
        let result = service.semantic_diagnostics(path);
        assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(
            semantic_fingerprint(&result),
            vec![missing_name(path, source, missing)],
            "{path}: {:#?}",
            result.diagnostics,
        );
    }

    for path in ["ordinary-missing.ts", "ordinary-missing.tsx"] {
        let source = "const kept: MissingOrdinarySibling = 1;\n";
        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source));
        let result = service.semantic_diagnostics(path);
        assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            semantic_fingerprint(&result),
            vec![missing_name(path, source, "MissingOrdinarySibling")],
            "{path}: {:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn generator_member_for_yields_inherit_the_typed_member_nonclaim() {
    let path = "generator-member-for-yield.ts";
    let source = concat!(
        "class Renamed { ",
        "*renamedItems() { for (const { value: renamed } of this) { yield renamed; } } ",
        "*nestedItems() { { for (const { value: changed } of this) { { yield changed; } } } } }\n",
        "function* declaredItems() { for (const { value: declared } of this) { yield declared; } }\n",
        "const expressionItems = function* changedItems() { for (const { value: wrapped } of this) { yield wrapped; } };\n",
        "const independent: MissingIndependent = 1;\n",
    );
    assert!(parser_codes(source).is_empty());

    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));
    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![missing_name(path, source, "MissingIndependent")],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn generator_function_expression_for_yield_keeps_its_typed_owner() {
    let path = "generator-expression-for-yield.ts";
    let source = concat!(
        "const expressionItems = function* renamedItems() { ",
        "for (const { value: wrapped } of this) { { yield wrapped; } } };\n",
        "const independent: MissingIndependent = 1;\n",
    );
    assert!(parser_codes(source).is_empty());

    let mut service = LanguageService::new(options());
    service.open(path, Arc::<str>::from(source));
    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![missing_name(path, source, "MissingIndependent")],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn opaque_class_expression_heads_follow_heritage_grammar_and_stay_local() {
    for (path, affected) in [
        (
            "anonymous-implements.ts",
            "interface RenamedContract {} const affected = class implements RenamedContract {};",
        ),
        (
            "implements-name.ts",
            "const affected = class implements {};",
        ),
        (
            "nested-generic-extends.ts",
            concat!(
                "declare class RenamedBase<First, Second = {}> {} ",
                "const affected = class Vessel<Element> extends ",
                "RenamedBase<Readonly<{ value: Element }> & Element, {}> {};",
            ),
        ),
        (
            "expression-heritage.ts",
            concat!(
                "declare const RenamedBase: unknown; ",
                "declare function wrap<Element>(value: unknown): any; ",
                "const affected = class Vessel<Element> extends wrap<Element>(RenamedBase) {};",
            ),
        ),
        (
            "multiple-implements.ts",
            concat!(
                "interface First<Element> {} interface Second {} ",
                "const affected = class Vessel<Element> implements First<Element>, Second {};",
            ),
        ),
    ] {
        let source = format!("{affected} const independent: MissingIndependent = 1;");
        assert!(parser_codes(&source).is_empty(), "{path}: {source}");

        let mut service = LanguageService::new(options());
        service.open(path, Arc::<str>::from(source.as_str()));
        let result = service.semantic_diagnostics(path);
        assert_eq!(
            result.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
        assert_eq!(
            semantic_fingerprint(&result),
            vec![missing_name(path, &source, "MissingIndependent")],
            "{path}: {:#?}",
            result.diagnostics,
        );
        assert_eq!(
            service.compile().exit_status,
            CompileExitStatus::SemanticIncomplete,
            "{path}"
        );
    }
}

#[test]
fn malformed_class_expression_heritage_still_reports_the_missing_body() {
    let source =
        "declare const RenamedBase: unknown; const broken = class Vessel extends RenamedBase";
    let source_text = SourceText::new(FileId(0), "missing-class-body.ts".into(), Arc::from(source));
    let parsed = parse_source(&source_text);
    let [diagnostic] = parsed.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", parsed.diagnostics);
    };
    assert_eq!(
        (
            diagnostic.code,
            diagnostic.start,
            diagnostic.length,
            diagnostic.message_text.as_str(),
        ),
        (1005, source.len() as u32, 0, "'{' expected."),
    );
}
