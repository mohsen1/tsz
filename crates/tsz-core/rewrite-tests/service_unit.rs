use super::*;
use crate::diagnostics::DiagnosticCategory;

#[macro_use]
#[path = "fixtures/service_query_expect.rs"]
mod service_query_expect;
expect_claimed_extension!();

fn index_value<T>(query: ServiceQuery<T>) -> T {
    query.expect_claimed("navigation service query")
}

#[test]
fn terminal_options_never_fabricate_checker_completion_under_no_check() {
    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "case.ts",
                Arc::<str>::from("const value = 1;"),
            )],
            &CompilerOptions {
                target: "es5".to_string(),
                no_check,
                no_emit: true,
                ..CompilerOptions::default()
            },
        );

        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [5108],
        );
        assert_eq!(
            output.check_file_completions,
            [SemanticCompletion::Deferred],
        );
        assert!(output.declaration_display_summaries.is_empty());
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

fn assert_navigation_nonclaimed(service: &LanguageService, path: &str, offset: u32) {
    let files = [path.to_string()];
    assert!(matches!(
        service.quick_info(path, offset),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
    assert!(matches!(
        service.definition_and_bound_span(path, offset),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
    assert!(matches!(
        service.references(path, offset),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
    assert!(matches!(
        service.document_highlights(path, offset, &files),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
    assert!(matches!(
        service.rename(path, offset),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
}

fn assert_navigation_claimed_negative(service: &LanguageService, path: &str, offset: u32) {
    assert!(index_value(service.quick_info(path, offset)).is_none());
    assert!(index_value(service.definition_and_bound_span(path, offset)).is_none());
    assert!(index_value(service.references(path, offset)).is_empty());
    assert!(index_value(service.document_highlights(path, offset, &[path.to_string()])).is_empty());
    let rename = index_value(service.rename(path, offset));
    assert!(!rename.info.can_rename);
    assert!(rename.locations.is_empty());
}

#[test]
fn diagnostic_products_follow_their_phase_instead_of_numeric_code_ranges() {
    let mut service = LanguageService::new(CompilerOptions {
        no_emit: true,
        ..CompilerOptions::default()
    });
    service.open("a-syntax.ts", Arc::<str>::from("const broken = ;"));
    service.open("z-semantic.ts", Arc::<str>::from("const missing: number;"));

    let syntax = service.syntactic_diagnostics("a-syntax.ts");
    assert_eq!(syntax.syntactic_completion, SemanticCompletion::Complete);
    assert_eq!(
        syntax
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.start,
                diagnostic.length,
                diagnostic.code,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![("a-syntax.ts", 15, 1, 1109, "Expression expected.")],
    );
    let semantic_syntax = service.syntactic_diagnostics("z-semantic.ts");
    assert_eq!(
        semantic_syntax.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert!(semantic_syntax.diagnostics.is_empty());

    let semantic = service.semantic_diagnostics("z-semantic.ts");
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        semantic
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.start,
                diagnostic.length,
                diagnostic.code,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            "z-semantic.ts",
            6,
            7,
            1155,
            "'const' declarations must be initialized.",
        )],
    );
    assert!(
        service
            .semantic_diagnostics("a-syntax.ts")
            .diagnostics
            .is_empty()
    );

    let output = service.compile();
    assert_eq!(output.diagnostics, syntax.diagnostics);

    let combined_source = "const broken = ;\nconst sibling: number;";
    service.open("m-combined.ts", Arc::<str>::from(combined_source));
    let combined_syntax = service.syntactic_diagnostics("m-combined.ts");
    assert_eq!(
        combined_syntax.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        combined_syntax
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [1109],
    );
    let combined = service.semantic_diagnostics("m-combined.ts");
    assert_eq!(combined.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        combined
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.start,
                diagnostic.length,
                diagnostic.code,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [(
            combined_source.find("sibling").unwrap() as u32,
            7,
            1155,
            DiagnosticCategory::Error,
            "'const' declarations must be initialized.",
            &[][..],
        )],
    );

    let mut missing_globals = LanguageService::new(CompilerOptions {
        no_emit: true,
        no_lib: true,
        ..CompilerOptions::default()
    });
    missing_globals.open("no-lib-syntax.ts", Arc::<str>::from("const broken = ;"));
    let missing_globals_syntax = missing_globals.syntactic_diagnostics("no-lib-syntax.ts");
    assert_eq!(
        missing_globals_syntax.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        missing_globals_syntax
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [1109],
    );
    let missing_semantic = missing_globals.semantic_diagnostics("no-lib-syntax.ts");
    assert!(missing_semantic.diagnostics.is_empty());
    assert_eq!(
        missing_semantic.semantic_completion,
        SemanticCompletion::Deferred
    );
    let missing_output = missing_globals.compile();
    assert_eq!(
        missing_output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [1109],
    );
    assert_eq!(
        missing_output.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        missing_output.exit_status,
        crate::CompileExitStatus::SemanticIncomplete
    );
}

#[test]
fn same_diagnostic_code_keeps_its_structural_phase_owner() {
    let mut service = LanguageService::new(CompilerOptions {
        no_emit: true,
        ..CompilerOptions::default()
    });
    service.open("syntax.ts", Arc::<str>::from(r"function \u0072eturn() {}"));
    service.open(
        "semantic.ts",
        Arc::<str>::from(r"async function f(\u0061wait: number) {}"),
    );

    assert_eq!(
        service
            .syntactic_diagnostics("syntax.ts")
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [1359],
    );
    assert!(
        service
            .semantic_diagnostics("syntax.ts")
            .diagnostics
            .is_empty()
    );
    let semantic_syntax = service.syntactic_diagnostics("semantic.ts");
    assert_eq!(
        semantic_syntax.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert!(semantic_syntax.diagnostics.is_empty());
    let semantic = service.semantic_diagnostics("semantic.ts");
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [1359],
    );

    let mut unchecked = LanguageService::new(CompilerOptions {
        no_check: true,
        no_emit: true,
        ..CompilerOptions::default()
    });
    unchecked.open(
        "semantic.ts",
        Arc::<str>::from(r"async function f(\u0061wait: number) {}"),
    );
    let unchecked_semantic = unchecked.semantic_diagnostics("semantic.ts");
    assert!(unchecked_semantic.diagnostics.is_empty());
    assert_eq!(
        unchecked_semantic.semantic_completion,
        SemanticCompletion::Complete
    );
}

#[test]
fn syntactic_diagnostic_completion_is_file_local_and_preserves_owned_facts() {
    let conditional = "function choose(flag:boolean){return flag ? 1 : 2;}";
    let namespace = "namespace Hidden { export const value = 1; }";
    let mixed = "namespace Wrapped {} const broken = ;";
    let independent = "const renamed = ;";
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("a-conditional.ts", Arc::<str>::from(conditional));
    service.open("b-namespace.ts", Arc::<str>::from(namespace));
    service.open("c-mixed.ts", Arc::<str>::from(mixed));
    service.open("z-independent.ts", Arc::<str>::from(independent));

    let conditional_result = service.syntactic_diagnostics("a-conditional.ts");
    assert_eq!(
        conditional_result.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert!(conditional_result.diagnostics.is_empty());

    let namespace_result = service.syntactic_diagnostics("b-namespace.ts");
    assert_eq!(
        namespace_result.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert!(namespace_result.diagnostics.is_empty());

    let mixed_result = service.syntactic_diagnostics("c-mixed.ts");
    assert_eq!(
        mixed_result.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        mixed_result
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [(
            "c-mixed.ts",
            1109,
            mixed.len() as u32 - 1,
            1,
            DiagnosticCategory::Error,
            "Expression expected.",
            &[][..],
        )],
    );

    let independent_result = service.syntactic_diagnostics("z-independent.ts");
    assert_eq!(
        independent_result.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        independent_result
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [(
            "z-independent.ts",
            1109,
            independent.len() as u32 - 1,
            1,
            DiagnosticCategory::Error,
            "Expression expected.",
            &[][..],
        )],
    );

    let mut aggregate = LanguageService::new(CompilerOptions {
        no_emit: true,
        ..CompilerOptions::default()
    });
    aggregate.open("a-host.ts", Arc::<str>::from(namespace));
    aggregate.open("z-semantic.ts", Arc::<str>::from("const sibling;"));
    let host_syntax = aggregate.syntactic_diagnostics("a-host.ts");
    assert_eq!(
        host_syntax.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert!(host_syntax.diagnostics.is_empty());
    let sibling_syntax = aggregate.syntactic_diagnostics("z-semantic.ts");
    assert_eq!(
        sibling_syntax.syntactic_completion,
        SemanticCompletion::Complete
    );
    assert!(sibling_syntax.diagnostics.is_empty());
    let sibling_semantic = aggregate.semantic_diagnostics("z-semantic.ts");
    assert_eq!(
        sibling_semantic.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        sibling_semantic
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                "z-semantic.ts",
                1155,
                6,
                7,
                DiagnosticCategory::Error,
                "'const' declarations must be initialized.",
                &[][..],
            ),
            (
                "z-semantic.ts",
                7005,
                6,
                7,
                DiagnosticCategory::Error,
                "Variable 'sibling' implicitly has an 'any' type.",
                &[][..],
            ),
        ],
    );
    let output = aggregate.compile();
    assert_eq!(output.diagnostics, sibling_semantic.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        output.exit_status,
        crate::CompileExitStatus::SemanticIncomplete
    );
}

#[test]
fn parser_recovery_syntax_nonclaims_survive_options_open_order_and_repeated_queries() {
    let generic = "declare const use: (...items:any[])=>void; use(<Cedar,>(): => 1);";
    let decorator = "class Holder { @decorate(\"x\", true) method() {} }\nconst exact = ;";
    let exact = "const ordinary = ;";
    let exact_class = "class Exact { method(value: ) {} }";
    let option_sets = [
        CompilerOptions::default(),
        CompilerOptions {
            no_check: true,
            ..CompilerOptions::default()
        },
        CompilerOptions {
            target: "renamed-invalid".to_string(),
            ..CompilerOptions::default()
        },
    ];
    let files = [
        ("a-generic.ts", generic),
        ("m-decorator.ts", decorator),
        ("y-exact-class.ts", exact_class),
        ("z-exact.ts", exact),
    ];

    for options in option_sets {
        for reversed in [false, true] {
            let mut service = LanguageService::new(options.clone());
            let mut order = files.to_vec();
            if reversed {
                order.reverse();
            }
            for (path, source) in order {
                service.open(path, Arc::<str>::from(source));
            }

            for _ in 0..2 {
                let generic_result = service.syntactic_diagnostics("a-generic.ts");
                assert_eq!(
                    generic_result.syntactic_completion,
                    SemanticCompletion::Deferred
                );

                let decorator_result = service.syntactic_diagnostics("m-decorator.ts");
                assert_eq!(
                    decorator_result.syntactic_completion,
                    SemanticCompletion::Deferred
                );
                assert!(decorator_result.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == 1109 && diagnostic.start == decorator.len() as u32 - 1
                }));

                let exact_result = service.syntactic_diagnostics("z-exact.ts");
                assert_eq!(
                    exact_result.syntactic_completion,
                    SemanticCompletion::Complete,
                    "target={} noCheck={} reversed={reversed}",
                    options.target,
                    options.no_check,
                );
                assert_eq!(
                    exact_result
                        .diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.code)
                        .collect::<Vec<_>>(),
                    [1109]
                );
                let exact_class_result = service.syntactic_diagnostics("y-exact-class.ts");
                assert_eq!(
                    exact_class_result.syntactic_completion,
                    SemanticCompletion::Complete
                );
                assert_eq!(
                    exact_class_result
                        .diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.code)
                        .collect::<Vec<_>>(),
                    [1110]
                );
            }
        }
    }
}

#[test]
fn compiled_snapshot_is_reused_and_invalidated_by_every_revision_owner() {
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open(
        "case.ts",
        Arc::<str>::from("const gap = `plain`; const value: string = missing;"),
    );
    assert!(service.compiled_snapshot.get_mut().is_none());

    let first = service.semantic_diagnostics("case.ts");
    assert_eq!(first.diagnostics.len(), 1);
    assert_eq!(first.semantic_completion, SemanticCompletion::Complete);
    assert!(service.compiled_snapshot.get_mut().is_some());
    let uncached = service.compile();
    let cached = service.compiled_snapshot.get_mut().as_ref().unwrap();
    assert_eq!(cached.semantic_completion, uncached.semantic_completion);
    assert_eq!(cached.diagnostics, uncached.diagnostics);

    service.configure(CompilerOptions {
        no_check: true,
        ..CompilerOptions::default()
    });
    assert!(service.compiled_snapshot.get_mut().is_none());
    let _ = service.semantic_diagnostics("case.ts");
    assert!(service.compiled_snapshot.get_mut().is_some());

    service.open("other.ts", Arc::<str>::from("const other = 1;"));
    assert!(service.compiled_snapshot.get_mut().is_none());
    let _ = service.quick_info("other.ts", 7);
    assert!(service.compiled_snapshot.get_mut().is_some());

    assert!(service.change("other.ts", Arc::<str>::from("const renamed = 1;")));
    assert!(service.compiled_snapshot.get_mut().is_none());
    let _ = service.quick_info("other.ts", 7);
    assert!(service.compiled_snapshot.get_mut().is_some());

    assert!(service.close("other.ts"));
    assert!(service.compiled_snapshot.get_mut().is_none());
    let _ = service.semantic_diagnostics("case.ts");
    assert!(service.compiled_snapshot.get_mut().is_some());

    service.reset();
    assert!(service.compiled_snapshot.get_mut().is_none());
}

#[test]
fn capability_scope_prefers_adjacent_starts_and_nested_right_edges() {
    let adjacent = "const g = `plain`;veryLongSiblingName;const veryLongSiblingName = 1;";
    let adjacent_reference = adjacent.find("veryLongSiblingName").unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("adjacent.ts", Arc::<str>::from(adjacent));

    let definition = service
        .definition_and_bound_span("adjacent.ts", adjacent_reference)
        .expect_claimed("adjacent statement navigation")
        .expect("the adjacent statement start must not inherit the prior nonclaim");
    assert_eq!(definition.definitions.len(), 1);
    assert_eq!(definition.definitions[0].name, "veryLongSiblingName");

    let nested = "function shell(bad: ){const sibling:string='x';sibling}";
    let nested_reference = nested.rfind("sibling").unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("nested.ts", Arc::<str>::from(nested));
    for offset in [nested_reference, nested_reference + "sibling".len() as u32] {
        let definition = service
            .definition_and_bound_span("nested.ts", offset)
            .expect_claimed("nested statement navigation")
            .expect("a nested statement owns both its token and right-edge query");
        assert_eq!(definition.definitions.len(), 1);
        assert_eq!(definition.definitions[0].name, "sibling");
    }
}

#[test]
fn quick_info_keeps_same_offset_merged_interfaces_across_root_orders() {
    let name_start = "interface ".len() as u32;
    for paths in [["alpha.ts", "omega.ts"], ["omega.ts", "alpha.ts"]] {
        let roots = paths
            .into_iter()
            .map(|path| {
                let source = match path {
                    "alpha.ts" => "interface Shared { alpha: number; }",
                    "omega.ts" => "interface Shared { omega: string; }",
                    _ => unreachable!(),
                };
                SourceInput::new(path, Arc::<str>::from(source))
            })
            .collect();
        let output = Compiler::new().compile(roots, &CompilerOptions::default());
        let index = navigation::NavigationIndex::build(&output);

        for path in paths {
            let info = index
                .quick_info(path, name_start)
                .expect("each merged declaration keeps its file-local quick info");
            assert_eq!(info.kind, "interface");
            assert_eq!(
                info.text_span,
                TextSpan {
                    start: name_start,
                    length: "Shared".len() as u32,
                }
            );
            assert_eq!(info.display, "interface Shared");
        }
    }
}

#[test]
fn incomplete_quick_info_result_keeps_navigation_identity_across_origins_and_roots() {
    let sources = [
        ("alpha.ts", "interface Shared<Value> { alpha: Value; }"),
        (
            "omega.ts",
            "interface Shared { omega: string; } let selected: Shared;",
        ),
    ];
    for reversed in [false, true] {
        let mut service = LanguageService::new(CompilerOptions::default());
        let ordered = if reversed {
            [sources[1], sources[0]]
        } else {
            sources
        };
        for (path, source) in ordered {
            service.open(path, Arc::<str>::from(source));
        }
        let origins = [
            ("alpha.ts", sources[0].1.find("Shared").unwrap() as u32),
            ("omega.ts", sources[1].1.find("Shared").unwrap() as u32),
            ("omega.ts", sources[1].1.rfind("Shared").unwrap() as u32),
        ];

        service.with_compiled_snapshot(|output| {
            for (path, offset) in origins {
                let file = compiled_file(output, path).expect("compiled service file");
                assert!(
                    output.capabilities.navigation_query_is_claimed(
                        Target::QuickInfo,
                        file,
                        offset,
                    ),
                    "checker completion must not be mirrored into capability analysis",
                );
            }
        });

        for _ in 0..2 {
            for (path, offset) in origins {
                assert!(matches!(
                    service.quick_info(path, offset),
                    ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
                ));

                let definition = service
                    .definition_and_bound_span(path, offset)
                    .expect_claimed("binder identity remains definitive")
                    .expect("merged interface definition");
                assert_eq!(definition.definitions.len(), 2);

                assert!(matches!(
                    service.references(path, offset),
                    ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
                ));

                let rename = service
                    .rename(path, offset)
                    .expect_claimed("rename does not depend on QuickInfo display");
                assert!(rename.info.can_rename);
                assert_eq!(rename.locations.len(), 3);
            }
        }
    }
}

#[test]
fn references_publish_only_exact_checker_display_across_roots_and_repeated_queries() {
    let sources = [
        (
            "alpha.ts",
            concat!(
                "function emptyControl() {} emptyControl; ",
                "function authoredControl(value: number): string { return ''; } authoredControl; ",
                "function inferredBody() { return 1; } inferredBody; ",
                "function genericCase<T>(value: T): T { return value; } genericCase; ",
                "function overloadedCase(value: string): string; ",
                "function overloadedCase(value: string) { return value; } overloadedCase; ",
                "async function asyncCase() {} asyncCase; ",
                "async function asyncAuthored(): Promise<void> {} asyncAuthored; ",
                "interface Shared<Value> { alpha: Value; }",
            ),
        ),
        (
            "omega.ts",
            concat!(
                "interface Shared<Value> { omega: Value; } let selected: Shared<number>; ",
                "emptyControl; authoredControl; inferredBody; genericCase; overloadedCase; ",
                "asyncCase; asyncAuthored;",
            ),
        ),
    ];
    for reversed in [false, true] {
        let mut service = LanguageService::new(CompilerOptions::default());
        let ordered = if reversed {
            [sources[1], sources[0]]
        } else {
            sources
        };
        for (path, source) in ordered {
            service.open(path, Arc::<str>::from(source));
        }

        for _ in 0..2 {
            for (name, expected, parts) in [
                (
                    "emptyControl",
                    "function emptyControl(): void",
                    vec![
                        ("function", "keyword"),
                        (" ", "space"),
                        ("emptyControl", "functionName"),
                        ("(", "punctuation"),
                        (")", "punctuation"),
                        (":", "punctuation"),
                        (" ", "space"),
                        ("void", "keyword"),
                    ],
                ),
                (
                    "authoredControl",
                    "function authoredControl(value: number): string",
                    vec![
                        ("function", "keyword"),
                        (" ", "space"),
                        ("authoredControl", "functionName"),
                        ("(", "punctuation"),
                        ("value", "parameterName"),
                        (":", "punctuation"),
                        (" ", "space"),
                        ("number", "keyword"),
                        (")", "punctuation"),
                        (":", "punctuation"),
                        (" ", "space"),
                        ("string", "keyword"),
                    ],
                ),
                (
                    "asyncAuthored",
                    "function asyncAuthored(): Promise<void>",
                    vec![
                        ("function", "keyword"),
                        (" ", "space"),
                        ("asyncAuthored", "functionName"),
                        ("(", "punctuation"),
                        (")", "punctuation"),
                        (":", "punctuation"),
                        (" ", "space"),
                        ("Promise<void>", "text"),
                    ],
                ),
            ] {
                for (path, offset) in [
                    ("alpha.ts", sources[0].1.find(name).unwrap() as u32),
                    ("alpha.ts", sources[0].1.rfind(name).unwrap() as u32),
                    ("omega.ts", sources[1].1.find(name).unwrap() as u32),
                ] {
                    let referenced = service
                        .references(path, offset)
                        .expect_claimed("complete reference display");
                    assert_eq!(referenced.len(), 1, "{path}:{name}");
                    assert_eq!(referenced[0].definition.name, expected, "{path}:{name}");
                    assert_eq!(
                        referenced[0]
                            .definition
                            .display_parts
                            .iter()
                            .map(|part| (part.text.as_str(), part.kind.as_str()))
                            .collect::<Vec<_>>(),
                        parts,
                        "{path}:{name}",
                    );
                }
            }

            for name in ["inferredBody", "genericCase", "overloadedCase", "asyncCase"] {
                for (path, offset) in [
                    ("alpha.ts", sources[0].1.find(name).unwrap() as u32),
                    ("alpha.ts", sources[0].1.rfind(name).unwrap() as u32),
                    ("omega.ts", sources[1].1.find(name).unwrap() as u32),
                ] {
                    assert!(matches!(
                        service.references(path, offset),
                        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
                    ));
                    assert!(
                        service
                            .definition_and_bound_span(path, offset)
                            .expect_claimed(
                                "binder identity is independent from references display",
                            )
                            .is_some()
                    );
                    assert!(
                        !service
                            .document_highlights(path, offset, &[])
                            .expect_claimed("highlights use binder identity")
                            .is_empty()
                    );
                    assert!(
                        service
                            .rename(path, offset)
                            .expect_claimed("rename identity")
                            .info
                            .can_rename
                    );
                }
            }

            for (path, offset) in [
                ("alpha.ts", sources[0].1.find("Shared").unwrap() as u32),
                ("omega.ts", sources[1].1.find("Shared").unwrap() as u32),
                ("omega.ts", sources[1].1.rfind("Shared").unwrap() as u32),
            ] {
                assert!(matches!(
                    service.references(path, offset),
                    ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
                ));
                assert!(
                    service
                        .definition_and_bound_span(path, offset)
                        .expect_claimed("merged interface binder identity")
                        .is_some()
                );
            }
        }
    }
}

#[test]
fn async_empty_function_quick_info_waits_for_checker_owned_promise_return() {
    let cases = [
        (
            "plain.ts",
            "function plain() {} plain;",
            "plain",
            Some("function plain(): void"),
        ),
        ("async.ts", "async function task() {} task;", "task", None),
        (
            "annotated.ts",
            "export async function promised(): Promise<void> {} promised;",
            "promised",
            Some("function promised(): Promise<void>"),
        ),
        (
            "nested.ts",
            "function outer() { async function renamed() {} renamed; }",
            "renamed",
            None,
        ),
    ];

    let mut service = LanguageService::new(CompilerOptions::default());
    for (path, source, _, _) in cases {
        service.open(path, Arc::<str>::from(source));
    }

    for (path, source, name, expected) in cases {
        for offset in [source.find(name).unwrap(), source.rfind(name).unwrap()] {
            let query = service.quick_info(path, offset as u32);
            match expected {
                Some(display) => assert_eq!(
                    query
                        .expect_claimed("complete authored or synchronous empty function summary")
                        .expect("function quick info")
                        .display,
                    display,
                    "{path}:{offset}",
                ),
                None => assert!(
                    matches!(
                        query,
                        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
                    ),
                    "{path}:{offset} {query:?}",
                ),
            }
        }
    }
    let mut no_lib = LanguageService::new(CompilerOptions {
        no_lib: true,
        ..CompilerOptions::default()
    });
    for (path, source, name, expected) in [
        (
            "no-lib-inferred.ts",
            "async function task() {} task;",
            "task",
            None,
        ),
        (
            "no-lib-authored.ts",
            "async function explicit(): Promise<void> {} explicit;",
            "explicit",
            Some("function explicit(): Promise<void>"),
        ),
    ] {
        no_lib.open(path, Arc::<str>::from(source));
        for offset in [source.find(name).unwrap(), source.rfind(name).unwrap()] {
            match expected {
                Some(display) => assert_eq!(
                    no_lib
                        .quick_info(path, offset as u32)
                        .expect_claimed("authored no-lib async summary")
                        .expect("function quick info")
                        .display,
                    display,
                ),
                None => assert!(matches!(
                    no_lib.quick_info(path, offset as u32),
                    ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
                )),
            }
        }
    }
}

#[test]
fn incomplete_reference_display_does_not_suppress_binder_identity_products() {
    let source = concat!(
        "function constrained<T extends string>(value: T): T { return value; } ",
        "constrained;",
    );
    let offset = source.rfind("constrained").unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("case.ts", Arc::<str>::from(source));

    assert!(matches!(
        service.references("case.ts", offset),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
    let definition = service
        .definition_and_bound_span("case.ts", offset)
        .expect_claimed("binder definition is independent from references display")
        .expect("function definition");
    assert_eq!(definition.definitions.len(), 1);
    assert_eq!(definition.definitions[0].name, "constrained");
    let rename = service
        .rename("case.ts", offset)
        .expect_claimed("rename identity is independent from references display");
    assert!(rename.info.can_rename);
    assert_eq!(rename.locations.len(), 2);
}

#[test]
fn quick_info_projects_local_var_through_bound_identity_without_widening_claims() {
    let source = concat!(
        "var global: string;",
        "function shell() { var local: string; var query: typeof local; local; ",
        "const fixed: string = 'x'; }",
    );
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("case.ts", Arc::<str>::from(source));

    for offset in [
        source.find("local").unwrap(),
        source.find("typeof local").unwrap() + "typeof ".len(),
        source.rfind("local").unwrap(),
    ] {
        let info = service
            .quick_info("case.ts", offset as u32)
            .expect_claimed("local variable quick info")
            .unwrap();
        assert_eq!(info.kind, "local var");
        assert_eq!(
            info.text_span,
            TextSpan {
                start: offset as u32,
                length: "local".len() as u32,
            }
        );
        assert_eq!(info.display, "(local var) local: string");
    }
    assert_eq!(
        service
            .quick_info("case.ts", source.find("global").unwrap() as u32)
            .expect_claimed("global variable quick info")
            .unwrap()
            .display,
        "var global: string"
    );
    assert_eq!(
        service
            .quick_info("case.ts", source.find("fixed").unwrap() as u32)
            .expect_claimed("fixed variable quick info")
            .unwrap()
            .display,
        "const fixed: string"
    );

    let exported = "export var publicValue: string; publicValue;";
    service.open("module.ts", Arc::<str>::from(exported));
    assert_eq!(
        service
            .quick_info("module.ts", exported.rfind("publicValue").unwrap() as u32)
            .expect_claimed("exported variable quick info")
            .unwrap()
            .display,
        "var publicValue: string"
    );
    let module_local = "var hidden: string; hidden; export {};";
    service.open("hidden.ts", Arc::<str>::from(module_local));
    assert_eq!(
        service
            .quick_info("hidden.ts", module_local.rfind("hidden").unwrap() as u32)
            .expect_claimed("module-local variable quick info")
            .unwrap()
            .display,
        "var hidden: string"
    );
    let block = "if (true) { var scoped: string; scoped; }";
    service.open("block.ts", Arc::<str>::from(block));
    for offset in [
        block.find("scoped").unwrap(),
        block.rfind("scoped").unwrap(),
    ] {
        let info = service
            .quick_info("block.ts", offset as u32)
            .expect_claimed("block variable quick info")
            .unwrap();
        assert_eq!(info.kind, "var");
        assert_eq!(info.display, "var scoped: string");
    }
    let alias = "import { publicValue as alias } from './module'; alias;";
    service.open("use.ts", Arc::<str>::from(alias));
    assert!(matches!(
        service.quick_info("use.ts", alias.rfind("alias").unwrap() as u32),
        ServiceQuery::Nonclaimed(_)
    ));

    let documented = "/** @type {number} */ const value = 1; value;";
    let mut js_service = LanguageService::new(CompilerOptions {
        allow_js: true,
        check_js: Some(true),
        no_emit: true,
        ..CompilerOptions::default()
    });
    js_service.open("documented.js", Arc::<str>::from(documented));
    for offset in [
        documented.find("value").unwrap(),
        documented.rfind("value").unwrap(),
    ] {
        let query = js_service.quick_info("documented.js", offset as u32);
        assert!(
            matches!(query, ServiceQuery::Nonclaimed(_)),
            "offset={offset} {query:?}"
        );
    }
}

#[test]
fn navigation_keys_follow_bound_declaration_groups_across_root_orders() {
    let sources = [
        (
            "class.ts",
            "class Dual {}\nnew Dual();\nlet instance: Dual;",
        ),
        (
            "interface.ts",
            "interface Dual { value: number }\nlet other: Dual;",
        ),
        (
            "enum.ts",
            "enum Recovered { Value }\nRecovered;\nlet recovered: Recovered;",
        ),
        (
            "meanings.ts",
            "interface Separate {}\nconst Separate = 1;\nlet typed: Separate;\nSeparate;",
        ),
        (
            "dep.ts",
            "export const Both = 1; export interface OnlyType {}",
        ),
        (
            "import.ts",
            concat!(
                "import { Both } from './dep';\n",
                "import type { OnlyType } from './dep';\n",
                "Both; let imported: Both; let typeOnly: OnlyType;",
            ),
        ),
        ("module-a.ts", "export const Local = 1; Local;"),
        ("module-b.ts", "export const Local = 2; Local;"),
        ("script-a.ts", "const Shared = 1; Shared;"),
        ("script-b.ts", "Shared;"),
    ];

    for reversed in [false, true] {
        let mut roots = sources
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect::<Vec<_>>();
        if reversed {
            roots.reverse();
        }
        let output = Compiler::new().compile(roots, &CompilerOptions::default());
        let index = navigation::NavigationIndex::build(&output);

        let class_source = sources[0].1;
        let class_reference = class_source.find("new Dual").unwrap() as u32 + 4;
        let class_definition = index
            .definition("class.ts", class_reference)
            .expect("the value side of a class keeps the merged declaration group");
        assert_eq!(class_definition.definitions.len(), 2);
        assert_eq!(
            class_definition
                .definitions
                .iter()
                .map(|definition| (definition.file_name.as_str(), definition.kind.as_str()))
                .collect::<Vec<_>>(),
            [("class.ts", "class"), ("interface.ts", "interface")]
        );
        let class_type_reference = class_source.rfind("Dual").unwrap() as u32 + 1;
        assert_eq!(
            index.references("class.ts", class_type_reference)[0]
                .references
                .len(),
            5
        );

        let enum_source = sources[2].1;
        let enum_type_reference = enum_source.rfind("Recovered").unwrap() as u32 + 1;
        let enum_definition = index
            .definition("enum.ts", enum_type_reference)
            .expect("the recovered enum type side shares its value-side authored span");
        assert_eq!(enum_definition.definitions.len(), 1);
        assert_eq!(enum_definition.definitions[0].kind, "module");
        assert_eq!(
            index.references("enum.ts", enum_type_reference)[0]
                .references
                .len(),
            3
        );

        let meanings_source = sources[3].1;
        let type_reference = meanings_source.find("typed: Separate").unwrap() as u32 + 8;
        let value_reference = meanings_source.rfind("Separate").unwrap() as u32 + 1;
        let type_definition = index.definition("meanings.ts", type_reference).unwrap();
        let value_definition = index.definition("meanings.ts", value_reference).unwrap();
        assert_eq!(type_definition.definitions.len(), 1);
        assert_eq!(type_definition.definitions[0].kind, "interface");
        assert_eq!(value_definition.definitions.len(), 1);
        assert_eq!(value_definition.definitions[0].kind, "const");
        assert_eq!(
            index.references("meanings.ts", type_reference)[0]
                .references
                .len(),
            2
        );
        assert_eq!(
            index.references("meanings.ts", value_reference)[0]
                .references
                .len(),
            2
        );

        let import_source = sources[5].1;
        let imported_type = import_source.find("imported: Both").unwrap() as u32 + 10;
        let type_only = import_source.rfind("OnlyType").unwrap() as u32 + 1;
        assert_eq!(
            index.references("import.ts", imported_type)[0]
                .references
                .len(),
            3
        );
        assert_eq!(
            index.references("import.ts", type_only)[0].references.len(),
            2
        );

        for (path, source) in [sources[6], sources[7]] {
            let reference = source.rfind("Local").unwrap() as u32 + 1;
            let references = &index.references(path, reference)[0].references;
            assert_eq!(references.len(), 2);
            assert!(
                references
                    .iter()
                    .all(|reference| reference.file_name == path)
            );
        }

        let shared_reference = sources[9].1.find("Shared").unwrap() as u32 + 1;
        let shared = &index.references("script-b.ts", shared_reference)[0].references;
        assert_eq!(shared.len(), 3);
        assert_eq!(
            shared
                .iter()
                .map(|reference| reference.file_name.as_str())
                .collect::<Vec<_>>(),
            ["script-a.ts", "script-a.ts", "script-b.ts"]
        );
    }
}

#[test]
fn rename_qualifies_non_local_external_module_declarations_from_every_occurrence() {
    let mut service = LanguageService::new(CompilerOptions::default());
    let class = concat!(
        "export default class RenamedClass {}\n",
        "let typed: RenamedClass;\n",
        "new RenamedClass();",
    );
    service.open("/workspace/class.ts", Arc::<str>::from(class));
    for (offset, _) in class.match_indices("RenamedClass") {
        let rename = service
            .rename("/workspace/class.ts", offset as u32)
            .expect_claimed("class rename");
        assert!(rename.info.can_rename);
        assert_eq!(rename.info.display_name.as_deref(), Some("RenamedClass"));
        assert_eq!(
            rename.info.full_display_name.as_deref(),
            Some("\"/workspace/class\".RenamedClass")
        );
        assert_eq!(rename.locations.len(), 3);
    }

    let function = concat!(
        "export default function chooseValue() {\n",
        "  return chooseValue;\n",
        "}",
    );
    service.open("/workspace/function.ts", Arc::<str>::from(function));
    for (offset, _) in function.match_indices("chooseValue") {
        let rename = service
            .rename("/workspace/function.ts", offset as u32)
            .expect_claimed("function rename");
        assert_eq!(
            rename.info.full_display_name.as_deref(),
            Some("\"/workspace/function\".chooseValue")
        );
        assert_eq!(rename.locations.len(), 2);
    }

    let named = "export function identity<T>(value: T): T { return value; } identity(1);";
    service.open("/workspace/named.ts", Arc::<str>::from(named));
    for (offset, _) in named.match_indices("identity") {
        assert_eq!(
            service
                .rename("/workspace/named.ts", offset as u32)
                .expect_claimed("named function rename")
                .info
                .full_display_name
                .as_deref(),
            Some("\"/workspace/named\".identity")
        );
    }

    let windows = "export const windowsName = 1; windowsName;";
    service.open(r"C:\workspace\windows.ts", Arc::<str>::from(windows));
    assert_eq!(
        service
            .rename(
                r"C:\workspace\windows.ts",
                windows.find("windowsName").unwrap() as u32
            )
            .expect_claimed("Windows-path rename")
            .info
            .full_display_name
            .as_deref(),
        Some("\"C:/workspace/windows\".windowsName")
    );
}

#[test]
fn rename_keeps_local_alias_global_and_default_expression_names_unqualified() {
    let cases = [
        (
            "/workspace/global.ts",
            "function globalName() {} globalName;",
            "globalName",
        ),
        (
            "/workspace/private.ts",
            "const hidden = 1; hidden; export {};",
            "hidden",
        ),
        (
            "/workspace/alias.ts",
            "let localName = 1; export { localName as outward }; localName;",
            "localName",
        ),
        (
            "/workspace/default.ts",
            "function keptLocal() { return keptLocal; } export default keptLocal; keptLocal;",
            "keptLocal",
        ),
    ];
    let mut service = LanguageService::new(CompilerOptions::default());
    for (path, source, name) in cases {
        service.open(path, Arc::<str>::from(source));
        for (offset, _) in source.match_indices(name) {
            let rename = service
                .rename(path, offset as u32)
                .expect_claimed("local rename");
            assert!(rename.info.can_rename, "{path}@{offset}");
            assert_eq!(rename.info.display_name.as_deref(), Some(name));
            assert_eq!(rename.info.full_display_name.as_deref(), Some(name));
        }
    }
}

#[test]
fn rename_module_qualification_removes_the_pinned_source_extension_family() {
    for (path, expected) in [
        ("/types/index.d.ts", "/types/index"),
        ("/types/index.d.mts", "/types/index"),
        ("/types/index.d.cts", "/types/index"),
        ("/src/file.mjs", "/src/file"),
        ("/src/file.mts", "/src/file"),
        ("/src/file.cjs", "/src/file"),
        ("/src/file.cts", "/src/file"),
        ("/src/file.ts", "/src/file"),
        ("/src/file.js", "/src/file"),
        ("/src/file.tsx", "/src/file"),
        ("/src/file.jsx", "/src/file"),
        ("/src/file.json", "/src/file"),
        ("/src/file.txt", "/src/file.txt"),
    ] {
        assert_eq!(remove_source_extension(path), expected);
    }
}

#[test]
fn navigation_ownership_uses_the_compiled_syntax_and_binder_snapshot() {
    let bound = "const boundName = 1; boundName;";
    let unresolved = "missingName;";
    let unmodeled = "const holder = { propertyName: 1 };";
    let touching_names = concat!(
        "interface Shape { defaultMember: string; 'quotedMember': number; 77: boolean; } ",
        "class Box { #privateMember = 1; defaultClass = 1; 'quotedClass' = 1; 88 = 1; } ",
        "const object = { 'quotedObject': 1, 99: 1 };",
    );
    let trivia = "const text = 'stringName'; // commentName\ntext;";
    let mut service = LanguageService::new(CompilerOptions::default());
    for (path, source) in [
        ("bound.ts", bound),
        ("unresolved.ts", unresolved),
        ("unmodeled.ts", unmodeled),
        ("touching.ts", touching_names),
        ("trivia.ts", trivia),
    ] {
        service.open(path, Arc::<str>::from(source));
    }

    for (offset, _) in bound.match_indices("boundName") {
        for position in [
            offset as u32,
            offset as u32 + 3,
            offset as u32 + "boundName".len() as u32,
        ] {
            assert!(
                index_value(service.quick_info("bound.ts", position)).is_some(),
                "bound quick info at {position}"
            );
            assert!(
                index_value(service.definition_and_bound_span("bound.ts", position)).is_some(),
                "bound definition at {position}"
            );
            let references = index_value(service.references("bound.ts", position));
            assert_eq!(references.len(), 1);
            assert_eq!(references[0].references.len(), 2);
            assert!(
                !index_value(service.document_highlights(
                    "bound.ts",
                    position,
                    &["bound.ts".to_string()],
                ))
                .is_empty()
            );
            assert!(
                index_value(service.rename("bound.ts", position))
                    .info
                    .can_rename
            );
        }
    }

    for position in [0, 3, "missingName".len() as u32] {
        assert_navigation_claimed_negative(&service, "unresolved.ts", position);
    }
    let property = unmodeled.find("propertyName").unwrap() as u32;
    for position in [
        property,
        property + 4,
        property + "propertyName".len() as u32,
    ] {
        assert_navigation_nonclaimed(&service, "unmodeled.ts", position);
    }
    for name in [
        "defaultMember",
        "'quotedMember'",
        "77",
        "#privateMember",
        "defaultClass",
        "'quotedClass'",
        "88",
        "'quotedObject'",
        "99",
    ] {
        let start = touching_names.find(name).unwrap() as u32;
        let end = start + name.len() as u32;
        for position in [start, start + (end - start) / 2, end] {
            assert_navigation_nonclaimed(&service, "touching.ts", position);
        }
    }

    for position in [
        trivia.find("stringName").unwrap() as u32 + 2,
        trivia.find("commentName").unwrap() as u32 + 2,
        trivia.find('=').unwrap() as u32,
        trivia.find(';').unwrap() as u32,
    ] {
        assert_navigation_claimed_negative(&service, "trivia.ts", position);
    }
}

#[test]
fn javascript_property_nonclaim_keeps_root_identity_independent_from_quick_info_summary() {
    let path = "rooted.js";
    let source = "const rooted={known:1}; rooted.value=1; rooted.value;";
    let root = source.find("rooted").unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions {
        allow_js: true,
        check_js: Some(true),
        no_emit: true,
        ..CompilerOptions::default()
    });
    service.open(path, Arc::<str>::from(source));

    service.with_compiled_snapshot(|output| {
        let file = compiled_file(output, path).expect("compiled JavaScript file");
        assert!(
            output
                .capabilities
                .navigation_query_is_claimed(Target::QuickInfo, file, root),
            "checker display completion must not mutate the structural QuickInfo claim",
        );
        for target in [
            Target::Definition,
            Target::References,
            Target::Highlights,
            Target::Rename,
        ] {
            assert!(
                output
                    .capabilities
                    .navigation_query_is_claimed(target, file, root),
                "the root binder identity remains claimed for {target:?}",
            );
        }
    });
    assert!(matches!(
        service.quick_info(path, root),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
    let definition = service
        .definition_and_bound_span(path, root)
        .expect_claimed("definition depends only on the claimed binder identity")
        .expect("JavaScript root definition");
    assert_eq!(definition.definitions.len(), 1);
    assert_eq!(definition.definitions[0].name, "rooted");
    assert!(matches!(
        service.references(path, root),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
    let rename = service
        .rename(path, root)
        .expect_claimed("rename depends only on the claimed binder identity");
    assert!(rename.info.can_rename);
    assert_eq!(rename.locations.len(), 3);
}

#[test]
fn navigation_identifier_facts_preserve_unicode_escapes_order_and_revisions() {
    let declaration = r"const café = 1; const \u0061lpha = café;";
    let use_site = r"café; \u0061lpha;";
    let unmodeled = r"const holder = { \u0068idden: 1 };";

    for reversed in [false, true] {
        let mut service = LanguageService::new(CompilerOptions::default());
        let mut sources = [
            ("declaration.ts", declaration),
            ("use.ts", use_site),
            ("unmodeled.ts", unmodeled),
        ];
        if reversed {
            sources.reverse();
        }
        for (path, source) in sources {
            service.open(path, Arc::<str>::from(source));
        }

        for _ in 0..2 {
            for raw_name in [r"café", r"\u0061lpha"] {
                let offset = use_site.find(raw_name).unwrap() as u32;
                for position in [offset, offset + 2, offset + raw_name.len() as u32] {
                    let definition =
                        index_value(service.definition_and_bound_span("use.ts", position))
                            .expect("cooked identifier identity resolves across files");
                    assert_eq!(definition.definitions.len(), 1);
                }
            }
            let hidden = unmodeled.find(r"\u0068idden").unwrap() as u32;
            assert_navigation_nonclaimed(&service, "unmodeled.ts", hidden + 3);
            assert_navigation_nonclaimed(
                &service,
                "unmodeled.ts",
                hidden + r"\u0068idden".len() as u32,
            );
        }

        assert!(service.change(
            "unmodeled.ts",
            Arc::<str>::from(r"const \u0068idden = 1; \u0068idden;"),
        ));
        let changed = service.text("unmodeled.ts").unwrap();
        let changed_reference = changed.rfind(r"\u0068idden").unwrap() as u32;
        assert!(
            index_value(service.definition_and_bound_span("unmodeled.ts", changed_reference))
                .is_some()
        );

        assert!(service.change(
            "unmodeled.ts",
            Arc::<str>::from("// hiddenName\nconst stable = 1;"),
        ));
        assert_navigation_claimed_negative(&service, "unmodeled.ts", 4);
    }
}

#[test]
fn navigation_source_does_not_reparse_for_query_ownership() {
    let source = include_str!("../src/service/navigation.rs");
    assert!(!source.contains("scan_source"));
}

#[test]
fn definition_container_kind_is_present_for_local_alias_and_default_targets() {
    let model = "export const remote = 1;";
    let usage = concat!(
        "import { remote as alias } from './model'; ",
        "function host() { const local = 1; local; } ",
        "alias;",
    );
    let defaulted = "export default class Defaulted {} new Defaulted();";
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("model.ts", Arc::<str>::from(model));
    service.open("use.ts", Arc::<str>::from(usage));
    service.open("default.ts", Arc::<str>::from(defaulted));

    for (path, source, marker, expected_name) in [
        ("use.ts", usage, "local;", "local"),
        ("use.ts", usage, "alias;", "remote"),
        ("default.ts", defaulted, "Defaulted();", "Defaulted"),
    ] {
        let position = source.rfind(marker).unwrap() as u32;
        let definition = index_value(service.definition_and_bound_span(path, position))
            .expect("definition target");
        assert_eq!(definition.definitions[0].name, expected_name);
        assert_eq!(definition.definitions[0].container_kind, "");
    }
}
