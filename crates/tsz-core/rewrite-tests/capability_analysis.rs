use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::service::{LanguageService, ServiceQuery};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

#[macro_use]
#[path = "fixtures/service_query_expect.rs"]
mod service_query_expect;
expect_claimed_extension!();

#[path = "capability_analysis_parts/support.rs"]
mod support;
use support::*;

#[path = "capability_analysis_parts/diagnostic_phases.rs"]
mod diagnostic_phases;

#[test]
fn plain_template_file_is_complete_and_preserves_unrelated_sibling_diagnostic() {
    let compiler = Compiler::new();
    let mut reversed = roots();
    reversed.reverse();

    let forward = compiler.compile(roots(), &CompilerOptions::default());
    let reverse = compiler.compile(reversed, &CompilerOptions::default());

    assert_eq!(forward.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        forward.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsGenerated
    );
    assert_eq!(
        emitted_fingerprint(&forward),
        vec![
            (
                "gap.js".to_string(),
                "\"use strict\";\nconst local = `plain`;\n".to_string(),
                false,
            ),
            (
                "sibling.js".to_string(),
                "\"use strict\";\nconst sibling = missingOwned;\n".to_string(),
                false,
            ),
        ]
    );
    assert!(forward.stats.types > 0, "the owned sibling must be checked");
    assert_eq!(
        diagnostic_fingerprint(&forward),
        vec![(
            "sibling.ts".to_string(),
            2304,
            24,
            12,
            DiagnosticCategory::Error,
            "Cannot find name 'missingOwned'.".to_string(),
            Vec::new(),
        )]
    );
    assert_eq!(
        diagnostic_fingerprint(&reverse),
        diagnostic_fingerprint(&forward)
    );
    assert_eq!(reverse.stats.types, forward.stats.types);
}

#[test]
fn no_check_keeps_owned_plain_template_program_complete() {
    let output = Compiler::new().compile(
        roots(),
        &CompilerOptions {
            no_check: true,
            ..CompilerOptions::default()
        },
    );

    assert!(output.diagnostics.is_empty());
    assert_eq!(output.stats.types, 0);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    assert_eq!(
        output
            .emitted_files
            .iter()
            .map(|file| (
                file.path.to_string_lossy().into_owned(),
                file.text.clone(),
                file.declaration,
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "gap.js".to_string(),
                "\"use strict\";\nconst local = `plain`;\n".to_string(),
                false,
            ),
            (
                "sibling.js".to_string(),
                "\"use strict\";\nconst sibling = missingOwned;\n".to_string(),
                false,
            ),
        ]
    );
}

#[test]
fn fatal_unknown_targets_do_not_execute_or_aggregate_emit_capabilities() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("const gap = `plain`; 1e_"),
        )],
        &CompilerOptions {
            target: "future".to_string(),
            ..CompilerOptions::default()
        },
    );

    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        diagnostic_fingerprint(&output),
        vec![(
            String::new(),
            6046,
            0,
            0,
            DiagnosticCategory::Error,
            concat!(
                "Argument for '--target' option must be: 'es6', 'es2015', 'es2016', ",
                "'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', ",
                "'es2023', 'es2024', 'es2025', 'esnext'."
            )
            .to_string(),
            Vec::new(),
        )]
    );
    assert!(output.emitted_files.is_empty());
}

#[test]
fn missing_essential_library_universe_keeps_aggregate_diagnostics_complete() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("const owned: string = 1;"),
        )],
        &CompilerOptions {
            no_lib: true,
            no_emit: true,
            ..CompilerOptions::default()
        },
    );

    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
    assert_eq!(output.diagnostics.len(), 10);
    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code == 2318)
    );
    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2322),
        "unowned checking cannot publish a fabricated relation diagnostic"
    );
}

#[test]
fn plain_template_cross_file_demands_are_complete_and_keep_exact_diagnostics() {
    let compiler = Compiler::new();
    let mut reversed = roots_with_cross_file_demand();
    reversed.reverse();

    for roots in [roots_with_cross_file_demand(), reversed] {
        let output = compiler.compile(roots, &CompilerOptions::default());
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsGenerated
        );
        assert_eq!(
            emitted_fingerprint(&output),
            vec![
                (
                    "gap.js".to_string(),
                    "\"use strict\";\nconst shared = 1;\nconst local = `plain`;\n".to_string(),
                    false,
                ),
                (
                    "sibling.js".to_string(),
                    "\"use strict\";\nconst copy = shared;\nconst sibling = missingOwned;\n"
                        .to_string(),
                    false,
                ),
            ]
        );
        assert!(output.stats.types > 0, "the sibling must still be checked");
        assert_eq!(
            diagnostic_fingerprint(&output),
            vec![
                (
                    "gap.ts".to_string(),
                    2322,
                    6,
                    6,
                    DiagnosticCategory::Error,
                    "Type 'number' is not assignable to type 'string'.".to_string(),
                    Vec::new(),
                ),
                (
                    "sibling.ts".to_string(),
                    2304,
                    53,
                    12,
                    DiagnosticCategory::Error,
                    "Cannot find name 'missingOwned'.".to_string(),
                    Vec::new(),
                ),
            ],
            "a literal gap must not suppress an owned sibling statement or declaration demand"
        );
    }
}

#[test]
fn global_groups_publish_independent_adjacency_without_selecting_a_nonclaimed_peer() {
    let compiler = Compiler::new();
    let mut reversed = roots_with_partially_nonclaimed_global_group();
    reversed.reverse();
    let mut fingerprints = None;

    for roots in [roots_with_partially_nonclaimed_global_group(), reversed] {
        let output = compiler.compile(roots, &CompilerOptions::default());
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.stats.types > 0);
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != 2345),
            "a partially nonclaimed binder group must not select a claimed peer"
        );
        assert!(output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == 2304
                && diagnostic.file == "consumer.ts"
                && diagnostic.message_text == "Cannot find name 'MissingOwned'."
        }));
        let current = diagnostic_fingerprint(&output);
        if let Some(expected) = &fingerprints {
            assert_eq!(&current, expected);
        } else {
            assert_eq!(
                current,
                vec![
                    (
                        "consumer.ts".to_string(),
                        2304,
                        37,
                        12,
                        DiagnosticCategory::Error,
                        "Cannot find name 'MissingOwned'.".to_string(),
                        Vec::new(),
                    ),
                    (
                        "declared.ts".to_string(),
                        2391,
                        9,
                        6,
                        DiagnosticCategory::Error,
                        "Function implementation is missing or not immediately following the declaration."
                            .to_string(),
                        Vec::new(),
                    ),
                ]
            );
            fingerprints = Some(current);
        }
    }
}

#[test]
fn plain_template_statement_is_complete_and_preserves_a_same_file_sibling() {
    let source = "const local = `plain`; const sibling: string = missingOwned;";
    for text in [
        source.to_string(),
        "const sibling: string = missingOwned; const local = `plain`;".to_string(),
    ] {
        let output = Compiler::new().compile(
            vec![SourceInput::new("mixed.ts", Arc::<str>::from(text))],
            &CompilerOptions::default(),
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        let diagnostic = output
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == 2304)
            .expect("owned sibling diagnostic");
        assert_eq!(diagnostic.file, "mixed.ts");
        assert_eq!(diagnostic.category, DiagnosticCategory::Error);
        assert_eq!(diagnostic.message_text, "Cannot find name 'missingOwned'.");
        assert!(diagnostic.related_information.is_empty());
    }
}

#[test]
fn template_recovery_extent_defers_attached_fragments_but_not_closed_siblings() {
    let cases = [
        (
            "tagged-tail.ts",
            "const tag: any = 0; tag `x`.escaped; const kept: MissingOwned = 1;",
            "MissingOwned",
        ),
        (
            "conditional-tail.ts",
            concat!(
                "type Tail<T> = T extends `${infer Renamed}` ? Escaped<Renamed> : never; ",
                "type Kept = MissingConditional;",
            ),
            "MissingConditional",
        ),
        (
            "brace-tail.ts",
            concat!(
                "function wrapper() { const tag: any = 0; tag `x`.escaped; } ",
                "const kept: MissingAfterBrace = 1;",
            ),
            "MissingAfterBrace",
        ),
    ];

    for (path, source, missing_name) in cases {
        let mut service = LanguageService::new(semantic_options());
        service.open(path, Arc::<str>::from(source));

        let result = service.semantic_diagnostics(path);
        assert_eq!(
            result.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
        assert_eq!(
            semantic_fingerprint(&result),
            vec![(
                path.to_string(),
                2304,
                source.find(missing_name).expect("sibling name") as u32,
                missing_name.len() as u32,
                DiagnosticCategory::Error,
                format!("Cannot find name '{missing_name}'."),
                Vec::new(),
            )],
            "{path}",
        );
        let output = service.compile();
        assert_eq!(
            output.exit_status,
            CompileExitStatus::SemanticIncomplete,
            "{path}: completion={:?}, diagnostics={:?}",
            output.semantic_completion,
            diagnostic_fingerprint(&output),
        );
    }
}

#[test]
fn every_literal_gap_preserves_same_file_semantic_and_required_type_siblings() {
    for source in [
        "const gap = 1e_; const kept: MissingOwned = 1;",
        "const kept: MissingOwned = 1; const gap = 1e_;",
        "const gap = { 1_0: 1 }; const kept: MissingOwned = 1;",
        "const kept: MissingOwned = 1; const gap = { 1_0: 1 };",
        "type Gap = 1_0; type Kept = MissingOwned;",
        "type Kept = MissingOwned; type Gap = 1_0;",
        "3enx; const kept: MissingOwned = 1;",
        "if (true) { const gap = 1e_ } const kept: MissingOwned = 1;",
        "function f(){ return 1e_; } const kept: MissingOwned = 1;",
    ] {
        assert_named_sibling_survives(source);
    }
}

#[test]
fn opaque_declaration_hosts_defer_only_dependent_names() {
    let cases = [
        (
            "opaque-host.ts",
            concat!(
                "declare namespace Vessel {\n",
                "    export const payload = `gap`;\n",
                "}\n",
                "Vessel.payload;\n",
                "const independent: MissingIndependent = 1;\n",
            ),
            vec![(
                "opaque-host.ts".to_string(),
                2304,
                98,
                18,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingIndependent'.".to_string(),
                Vec::new(),
            )],
        ),
        (
            "nested-host.ts",
            concat!(
                "const before: MissingBefore = 1;\n",
                "declare namespace CrateBox {\n",
                "    export namespace InnerBox {\n",
                "        export const payload = `gap`;\n",
                "    }\n",
                "}\n",
                "CrateBox.InnerBox.payload;\n",
                "const after: MissingAfter = 1;\n",
            ),
            vec![
                (
                    "nested-host.ts".to_string(),
                    2304,
                    14,
                    13,
                    DiagnosticCategory::Error,
                    "Cannot find name 'MissingBefore'.".to_string(),
                    Vec::new(),
                ),
                (
                    "nested-host.ts".to_string(),
                    2304,
                    180,
                    12,
                    DiagnosticCategory::Error,
                    "Cannot find name 'MissingAfter'.".to_string(),
                    Vec::new(),
                ),
            ],
        ),
    ];

    for (path, source, expected) in cases {
        let mut service = LanguageService::new(semantic_options());
        service.open(path, Arc::<str>::from(source));

        let result = service.semantic_diagnostics(path);
        assert_eq!(
            result.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
        assert_eq!(semantic_fingerprint(&result), expected, "{path}");
        assert_eq!(
            service.compile().exit_status,
            CompileExitStatus::SemanticIncomplete,
            "{path}"
        );
    }
}

#[test]
fn opaque_declaration_host_body_defers_internal_references_only() {
    let source = concat!(
        "const before: MissingBefore = 1;\n",
        "declare namespace Box {\n",
        "    enum Inner {\n",
        "        a = `gap`\n",
        "    }\n",
        "    export const x = Inner.a;\n",
        "}\n",
        "Box.x;\n",
        "const after: MissingAfter = 1;\n",
    );
    let mut service = LanguageService::new(semantic_options());
    service.open("host-body.ts", Arc::<str>::from(source));

    let result = service.semantic_diagnostics("host-body.ts");
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![
            (
                "host-body.ts".to_string(),
                2304,
                14,
                13,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingBefore'.".to_string(),
                Vec::new(),
            ),
            (
                "host-body.ts".to_string(),
                2304,
                150,
                12,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingAfter'.".to_string(),
                Vec::new(),
            ),
        ]
    );
    assert_eq!(
        service.compile().exit_status,
        CompileExitStatus::SemanticIncomplete
    );
}

#[test]
fn opaque_global_host_closes_cross_file_demands_without_poisoning_siblings() {
    let producer_source = concat!(
        "declare namespace PackageBox {\n",
        "    export const payload = `gap`;\n",
        "}\n",
    );
    let consumer_source = "PackageBox.payload;\nconst sameFile: MissingConsumer = 1;\n";
    let safe_source = "const crossFile: MissingSafe = 1;\n";

    for (producer_path, consumer_path) in [
        ("a-producer.ts", "z-consumer.ts"),
        ("z-producer.ts", "a-consumer.ts"),
    ] {
        let mut service = LanguageService::new(semantic_options());
        service.open(producer_path, Arc::<str>::from(producer_source));
        service.open(consumer_path, Arc::<str>::from(consumer_source));
        service.open("m-safe.ts", Arc::<str>::from(safe_source));

        let producer = service.semantic_diagnostics(producer_path);
        assert_eq!(
            producer.semantic_completion,
            SemanticCompletion::Deferred,
            "{producer_path}"
        );
        assert!(
            semantic_fingerprint(&producer).is_empty(),
            "{producer_path}: {:#?}",
            producer.diagnostics
        );

        let consumer = service.semantic_diagnostics(consumer_path);
        assert_eq!(
            consumer.semantic_completion,
            SemanticCompletion::Deferred,
            "{consumer_path}: {:#?}",
            semantic_fingerprint(&consumer),
        );
        assert_eq!(
            semantic_fingerprint(&consumer),
            vec![(
                consumer_path.to_string(),
                2304,
                36,
                15,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingConsumer'.".to_string(),
                Vec::new(),
            )],
            "{consumer_path}"
        );

        let safe = service.semantic_diagnostics("m-safe.ts");
        assert_eq!(
            safe.semantic_completion,
            SemanticCompletion::Complete,
            "{producer_path} -> {consumer_path}"
        );
        assert_eq!(
            semantic_fingerprint(&safe),
            vec![(
                "m-safe.ts".to_string(),
                2304,
                17,
                11,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingSafe'.".to_string(),
                Vec::new(),
            )]
        );

        let output = service.compile();
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn direct_opaque_global_name_demand_propagates_without_forcing() {
    let producer_source = concat!(
        "declare namespace PackageBox {\n",
        "    export const payload = `gap`;\n",
        "}\n",
    );
    let consumer_source = "const copy = PackageBox;\nconst sameFile: MissingConsumer = 1;\n";

    for (producer_path, consumer_path) in [
        ("a-producer.ts", "z-consumer.ts"),
        ("z-producer.ts", "a-consumer.ts"),
    ] {
        let mut service = LanguageService::new(semantic_options());
        service.open(producer_path, Arc::<str>::from(producer_source));
        service.open(consumer_path, Arc::<str>::from(consumer_source));

        let producer = service.semantic_diagnostics(producer_path);
        assert_eq!(
            producer.semantic_completion,
            SemanticCompletion::Deferred,
            "{producer_path}"
        );
        assert!(
            semantic_fingerprint(&producer).is_empty(),
            "{producer_path}: {:#?}",
            producer.diagnostics
        );

        let consumer = service.semantic_diagnostics(consumer_path);
        assert_eq!(
            consumer.semantic_completion,
            SemanticCompletion::Deferred,
            "{consumer_path}: {:#?}",
            semantic_fingerprint(&consumer),
        );
        assert_eq!(
            semantic_fingerprint(&consumer),
            vec![(
                consumer_path.to_string(),
                2304,
                41,
                15,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingConsumer'.".to_string(),
                Vec::new(),
            )],
            "{consumer_path}"
        );

        let output = service.compile();
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn incomplete_element_access_switch_defers_exhaustiveness_relation_only() {
    let source = concat!(
        "type Variant = LeftCase | RightCase;\n",
        "interface LeftCase { tag: 'left'; left: number; }\n",
        "interface RightCase { tag: 'right'; right: string; }\n",
        "declare function unreachable(value: never): never;\n",
        "function elementGap(value: Variant) {\n",
        "    switch (value['tag']) {\n",
        "        case 'left': return value.left;\n",
        "        case 'right': return value.right;\n",
        "        default: return unreachable(value);\n",
        "    }\n",
        "}\n",
        "const independent: MissingIndependent = 1;\n",
    );
    let mut service = LanguageService::new(semantic_options());
    service.open("switch-flow.ts", Arc::<str>::from(source));

    let result = service.semantic_diagnostics("switch-flow.ts");
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![(
            "switch-flow.ts".to_string(),
            2304,
            410,
            18,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingIndependent'.".to_string(),
            Vec::new(),
        )]
    );
    assert_eq!(
        service.compile().exit_status,
        CompileExitStatus::SemanticIncomplete
    );
}

#[test]
fn quick_info_uses_completed_plain_templates_without_poisoning_unsupported_siblings() {
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("gap.ts", Arc::<str>::from("const local = `plain`;"));
    service.open(
        "sibling.ts",
        Arc::<str>::from("const sibling: string = missingOwned;"),
    );

    let gap = service
        .quick_info("gap.ts", 7)
        .expect_claimed("plain template quick info capability")
        .expect("plain template quick info is definitive");
    assert_eq!(gap.display, "const local: \"plain\"");
    let sibling = service
        .quick_info("sibling.ts", 7)
        .expect_claimed("sibling quick info capability")
        .expect("unrelated sibling quick info remains definitive");
    assert_eq!(sibling.display, "const sibling: string");

    let diagnostics = service.semantic_diagnostics("sibling.ts");
    assert_eq!(
        diagnostics.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(diagnostics.diagnostics.len(), 1);
    assert_eq!(diagnostics.diagnostics[0].code, 2304);

    service.change("gap.ts", Arc::<str>::from("const local = 'plain';"));
    assert!(
        service
            .quick_info("gap.ts", 7)
            .expect_claimed("revised quick info")
            .is_some(),
        "a source revision must invalidate the compiled capability snapshot"
    );

    let mut completion_service = LanguageService::new(CompilerOptions::default());
    completion_service.open(
        "deferred.ts",
        Arc::<str>::from("let direct:{value:number[string]};"),
    );
    completion_service.open(
        "safe.ts",
        Arc::<str>::from("const kept: string = MissingOwned;"),
    );
    let safe_diagnostics = completion_service.semantic_diagnostics("safe.ts");
    assert_eq!(
        safe_diagnostics.semantic_completion,
        SemanticCompletion::Complete,
        "an unrelated checker deferral must not poison a file-local service answer"
    );
    assert_eq!(
        safe_diagnostics
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![2304]
    );
    assert_eq!(
        completion_service
            .semantic_diagnostics("deferred.ts")
            .semantic_completion,
        SemanticCompletion::Deferred
    );

    let mixed = "const local = `plain`; const sibling: string = 'owned';";
    let mut mixed_service = LanguageService::new(CompilerOptions::default());
    mixed_service.open("mixed.ts", Arc::<str>::from(mixed));
    let local_offset = mixed.find("local").expect("local") as u32;
    let sibling_offset = mixed.find("sibling").expect("sibling") as u32;
    assert_eq!(
        mixed_service
            .quick_info("mixed.ts", local_offset)
            .expect_claimed("mixed local quick info capability")
            .expect("plain template quick info")
            .display,
        "const local: \"plain\""
    );
    assert_eq!(
        mixed_service
            .quick_info("mixed.ts", sibling_offset)
            .expect_claimed("mixed sibling quick info capability")
            .expect("same-file sibling quick info")
            .display,
        "const sibling: string"
    );
    assert!(
        mixed_service
            .definition_and_bound_span("mixed.ts", local_offset)
            .expect_claimed("mixed local definition")
            .is_some()
    );
    assert!(
        mixed_service
            .definition_and_bound_span("mixed.ts", sibling_offset)
            .expect_claimed("mixed sibling definition")
            .is_some()
    );
    assert!(matches!(
        mixed_service.references("mixed.ts", local_offset),
        tsz::service::ServiceQuery::Nonclaimed(_)
    ));
    let sibling_references = mixed_service
        .references("mixed.ts", sibling_offset)
        .expect_claimed("authored sibling reference display");
    assert_eq!(sibling_references.len(), 1);
    assert_eq!(
        sibling_references[0].definition.name,
        "const sibling: string"
    );
    assert_eq!(
        sibling_references[0]
            .definition
            .display_parts
            .iter()
            .map(|part| (part.text.as_str(), part.kind.as_str()))
            .collect::<Vec<_>>(),
        [
            ("const", "keyword"),
            (" ", "space"),
            ("sibling", "localName"),
            (":", "punctuation"),
            (" ", "space"),
            ("string", "keyword"),
        ]
    );
    assert!(
        !mixed_service
            .document_highlights("mixed.ts", local_offset, &["mixed.ts".to_string()])
            .expect_claimed("mixed local highlights")
            .is_empty()
    );
    assert!(
        !mixed_service
            .document_highlights("mixed.ts", sibling_offset, &["mixed.ts".to_string()])
            .expect_claimed("mixed sibling highlights")
            .is_empty()
    );
    assert!(
        mixed_service
            .rename("mixed.ts", local_offset)
            .expect_claimed("mixed local rename")
            .info
            .can_rename
    );
    assert!(
        mixed_service
            .rename("mixed.ts", sibling_offset)
            .expect_claimed("mixed sibling rename")
            .info
            .can_rename
    );

    let reference_source = "interface Box {} function useValue(value: Box) { return value; }";
    let mut reference_service = LanguageService::new(CompilerOptions::default());
    reference_service.open("references.ts", Arc::<str>::from(reference_source));
    let parameter = reference_source.find("value: Box").expect("parameter") as u32;
    assert!(matches!(
        reference_service.quick_info("references.ts", parameter),
        tsz::service::ServiceQuery::Nonclaimed(_)
    ));
    let references = reference_service
        .references("references.ts", parameter)
        .expect_claimed("annotated parameter has a complete reference display");
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].references.len(), 2);
    assert_eq!(references[0].definition.name, "(parameter) value: Box");
    assert_eq!(
        references[0]
            .definition
            .display_parts
            .iter()
            .map(|part| (part.text.as_str(), part.kind.as_str()))
            .collect::<Vec<_>>(),
        [
            ("(", "punctuation"),
            ("parameter", "text"),
            (")", "punctuation"),
            (" ", "space"),
            ("value", "parameterName"),
            (":", "punctuation"),
            (" ", "space"),
            ("Box", "text"),
        ]
    );

    let result_scope_source = "const safe = 1; const gap = `${safe}`; const useSafe = safe;";
    let mut result_scope_service = LanguageService::new(CompilerOptions::default());
    result_scope_service.open("results.ts", Arc::<str>::from(result_scope_source));
    let safe_declaration = result_scope_source.find("safe").expect("safe") as u32;
    assert!(matches!(
        result_scope_service.references("results.ts", safe_declaration),
        tsz::service::ServiceQuery::Nonclaimed(_)
    ));
    assert!(
        !result_scope_service
            .document_highlights("results.ts", safe_declaration, &["results.ts".to_string()])
            .expect_claimed("template substitution highlights")
            .is_empty()
    );
    assert!(
        result_scope_service
            .rename("results.ts", safe_declaration)
            .expect_claimed("template substitution rename")
            .info
            .can_rename
    );

    let literal_scope_source = "const safe = 1; const gap = `${\"safe\"}`; const useSafe = safe;";
    let mut literal_scope_service = LanguageService::new(CompilerOptions::default());
    literal_scope_service.open("literal-results.ts", Arc::<str>::from(literal_scope_source));
    let literal_safe = literal_scope_source.find("safe").expect("safe") as u32;
    assert!(matches!(
        literal_scope_service.references("literal-results.ts", literal_safe),
        tsz::service::ServiceQuery::Nonclaimed(_)
    ));
    assert!(
        literal_scope_service
            .rename("literal-results.ts", literal_safe)
            .expect_claimed("literal substitution rename")
            .info
            .can_rename
    );

    let mut cross_file_service = LanguageService::new(CompilerOptions::default());
    cross_file_service.open("declaration.ts", Arc::<str>::from("const shared = 1;"));
    cross_file_service.open(
        "template-use.ts",
        Arc::<str>::from("const gap = `${shared}`;"),
    );
    assert!(matches!(
        cross_file_service.references("declaration.ts", "const ".len() as u32),
        tsz::service::ServiceQuery::Nonclaimed(_)
    ));
    assert!(
        cross_file_service
            .rename("declaration.ts", "const ".len() as u32)
            .expect_claimed("cross-file template rename")
            .info
            .can_rename
    );

    let nonclaimed_declaration_source = "const gapOwned = `plain`; const useGap = gapOwned;";
    let mut declaration_scope_service = LanguageService::new(CompilerOptions::default());
    declaration_scope_service.open(
        "declaration.ts",
        Arc::<str>::from(nonclaimed_declaration_source),
    );
    let use_gap = nonclaimed_declaration_source
        .rfind("gapOwned")
        .expect("gapOwned reference") as u32;
    assert!(
        declaration_scope_service
            .definition_and_bound_span("declaration.ts", use_gap)
            .expect_claimed("template declaration navigation")
            .is_some(),
        "a plain template declaration publishes its owned definition"
    );

    let module_group = concat!(
        "export {}; ",
        "function shared(value: string): string; ",
        "function shared(value: string) { return `plain`; }",
    );
    let mut module_group_service = LanguageService::new(CompilerOptions::default());
    module_group_service.open("module.ts", Arc::<str>::from(module_group));
    let signature = module_group.find("shared").expect("signature") as u32;
    assert!(
        module_group_service
            .definition_and_bound_span("module.ts", signature)
            .expect_claimed("module group navigation")
            .is_some(),
        "a modeled implementation keeps module-local binder navigation claimed"
    );
    assert!(
        module_group_service
            .rename("module.ts", signature)
            .expect_claimed("module group rename")
            .info
            .can_rename
    );
}

#[test]
fn plain_template_service_completion_is_complete_across_file_orders() {
    for (gap_path, consumer_path) in [("a-gap.ts", "z-consumer.ts"), ("z-gap.ts", "a-consumer.ts")]
    {
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open(
            gap_path,
            Arc::<str>::from("const shared: string = `plain`;"),
        );
        service.open(
            consumer_path,
            Arc::<str>::from("const copy: string = shared;"),
        );
        service.open(
            "m-safe.ts",
            Arc::<str>::from("const kept: string = MissingOwned;"),
        );

        let gap = service.semantic_diagnostics(gap_path);
        assert_eq!(gap.semantic_completion, SemanticCompletion::Complete);
        assert!(gap.diagnostics.is_empty());

        let consumer = service.semantic_diagnostics(consumer_path);
        assert_eq!(consumer.semantic_completion, SemanticCompletion::Complete);
        assert!(consumer.diagnostics.is_empty());

        let safe = service.semantic_diagnostics("m-safe.ts");
        assert_eq!(safe.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(
            safe.diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.category,
                    diagnostic.message_text.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![(
                2304,
                21,
                12,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingOwned'.",
            )]
        );
        assert!(safe.diagnostics[0].related_information.is_empty());
    }
}

#[test]
fn represented_nested_templates_preserve_inner_required_type_siblings() {
    let cases = [
        (
            "nested-template.ts",
            concat!(
                "function wrapper() {\n",
                "  const gap: string = `head${\"value\"}tail`;\n",
                "  const dependent: number = gap;\n",
                "  const kept: MissingInside = 1;\n",
                "}\n",
            ),
        ),
        (
            "renamed-template.ts",
            concat!(
                "function enclosure() {\n",
                "  const recovered: string = `left${\"middle\"}right`;\n",
                "  const renamedUse: number = recovered;\n",
                "  const survivor: MissingInside = 1;\n",
                "}\n",
            ),
        ),
        (
            "wrapped-template.ts",
            concat!(
                "function wrapped() {\n",
                "  {\n",
                "    const nestedGap: string = `before${\"inside\"}after`;\n",
                "    const nestedUse: number = nestedGap;\n",
                "    const nestedKept: MissingInside = 1;\n",
                "  }\n",
                "}\n",
            ),
        ),
        (
            "repeated-template.ts",
            concat!(
                "function repeated() {\n",
                "  const firstGap: string = `first${\"value\"}tail`;\n",
                "  const firstUse: number = firstGap;\n",
                "  const between: MissingInside = 1;\n",
                "  const secondGap: string = `second${\"value\"}tail`;\n",
                "  const secondUse: number = secondGap;\n",
                "}\n",
            ),
        ),
    ];

    for (path, source) in cases {
        let mut service = LanguageService::new(semantic_options());
        service.open(path, Arc::<str>::from(source));

        for _ in 0..2 {
            let result = service.semantic_diagnostics(path);
            assert_eq!(
                result.semantic_completion,
                SemanticCompletion::Deferred,
                "{path}"
            );
            let mut expected = [
                "dependent",
                "renamedUse",
                "nestedUse",
                "firstUse",
                "secondUse",
            ]
            .into_iter()
            .filter_map(|name| {
                source.find(name).map(|start| {
                    (
                        path.to_string(),
                        2322,
                        start as u32,
                        name.len() as u32,
                        DiagnosticCategory::Error,
                        "Type 'string' is not assignable to type 'number'.".to_string(),
                        Vec::new(),
                    )
                })
            })
            .collect::<Vec<_>>();
            expected.push((
                path.to_string(),
                2304,
                source.find("MissingInside").expect("required-type sibling") as u32,
                "MissingInside".len() as u32,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingInside'.".to_string(),
                Vec::new(),
            ));
            expected.sort_by_key(|diagnostic| diagnostic.2);
            assert_eq!(
                semantic_fingerprint(&result),
                expected,
                "{path}: {:#?}",
                result.diagnostics,
            );
        }

        assert_eq!(
            service.compile().exit_status,
            CompileExitStatus::SemanticIncomplete,
            "{path}"
        );
    }
}

#[test]
fn represented_nested_templates_preserve_service_query_origins() {
    let source = concat!(
        "function QueryShell() {\n",
        "  {\n",
        "    const broken: string = `head${\"value\"}tail`;\n",
        "    const nestedSibling: string = 'owned';\n",
        "    nestedSibling;\n",
        "  }\n",
        "}\n",
    );
    let path = "nested-service.ts";
    let broken = source.find("broken").expect("recovered declaration") as u32;
    let sibling = source.find("nestedSibling").expect("claimed declaration") as u32;
    let sibling_use = source.rfind("nestedSibling").expect("claimed use") as u32;
    let mut service = LanguageService::new(semantic_options());
    service.open(path, Arc::<str>::from(source));

    for _ in 0..2 {
        assert_eq!(
            service
                .quick_info(path, broken)
                .expect_claimed("annotated recovered quick info capability")
                .expect("annotated template declaration quick info")
                .display,
            "const broken: string"
        );
        assert!(
            service
                .definition_and_bound_span(path, broken)
                .expect_claimed("annotated recovered definition")
                .is_some()
        );
        assert!(
            !service
                .references(path, broken)
                .expect_claimed("annotated recovered references")
                .is_empty()
        );
        assert!(
            !service
                .document_highlights(path, broken, &[path.to_string()])
                .expect_claimed("annotated recovered highlights")
                .is_empty()
        );
        assert!(
            service
                .rename(path, broken)
                .expect_claimed("annotated recovered rename")
                .info
                .can_rename
        );

        let quick_info = service
            .quick_info(path, sibling)
            .expect_claimed("nested sibling quick info capability")
            .expect("nested sibling quick info remains claimed");
        assert_eq!(quick_info.kind, "const");
        assert_eq!(quick_info.text_span.start, sibling);
        assert_eq!(quick_info.text_span.length, "nestedSibling".len() as u32);
        assert_eq!(quick_info.display, "const nestedSibling: string");

        let definition = service
            .definition_and_bound_span(path, sibling_use)
            .expect_claimed("nested sibling definition capability")
            .expect("nested sibling definition remains claimed");
        assert_eq!(definition.text_span.start, sibling_use);
        assert_eq!(definition.text_span.length, "nestedSibling".len() as u32);
        assert_eq!(definition.definitions.len(), 1);
        assert_eq!(definition.definitions[0].file_name, path);
        assert_eq!(definition.definitions[0].name, "nestedSibling");
        assert_eq!(definition.definitions[0].text_span.start, sibling);

        let references = service
            .references(path, sibling)
            .expect_claimed("nested sibling references");
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].definition.name, "const nestedSibling: string");
        assert_eq!(
            references[0]
                .references
                .iter()
                .map(|reference| reference.text_span.start)
                .collect::<Vec<_>>(),
            vec![sibling, sibling_use]
        );

        let highlights = service
            .document_highlights(path, sibling, &[path.to_string()])
            .expect_claimed("nested sibling highlights");
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].file_name, path);
        assert_eq!(
            highlights[0]
                .highlight_spans
                .iter()
                .map(|highlight| highlight.text_span.start)
                .collect::<Vec<_>>(),
            vec![sibling, sibling_use]
        );

        let rename = service
            .rename(path, sibling)
            .expect_claimed("nested sibling rename");
        assert!(rename.info.can_rename);
        assert_eq!(rename.info.display_name.as_deref(), Some("nestedSibling"));
        assert_eq!(
            rename
                .locations
                .iter()
                .map(|location| location.text_span.start)
                .collect::<Vec<_>>(),
            vec![sibling, sibling_use]
        );
    }

    let edge_source = concat!(
        "const edgeGap: string = `head${\"value\"}tail`;\n",
        "const edgeSibling: string = 'owned';\n",
        "edgeSibling",
    );
    let edge_declaration = edge_source.find("edgeSibling").expect("edge declaration") as u32;
    let edge_use = edge_source.rfind("edgeSibling").expect("edge use") as u32;
    let mut edge_service = LanguageService::new(semantic_options());
    edge_service.open("edge-service.ts", Arc::<str>::from(edge_source));
    for offset in [edge_use, edge_use + "edgeSibling".len() as u32] {
        let definition = edge_service
            .definition_and_bound_span("edge-service.ts", offset)
            .expect_claimed("right-edge definition capability")
            .expect("right-edge query keeps its statement owner");
        assert_eq!(definition.definitions.len(), 1);
        assert_eq!(definition.definitions[0].name, "edgeSibling");
        assert_eq!(definition.definitions[0].text_span.start, edge_declaration);
        let references = edge_service
            .references("edge-service.ts", offset)
            .expect_claimed("right-edge references");
        assert_eq!(references.len(), 1);
        assert_eq!(
            references[0]
                .references
                .iter()
                .map(|reference| reference.text_span.start)
                .collect::<Vec<_>>(),
            vec![edge_declaration, edge_use]
        );
    }
}

#[test]
fn recovered_signature_containers_keep_required_type_descendants_only() {
    let cases = [
        (
            "function-container.ts",
            concat!(
                "function wrapper(value: ) {\n",
                "  type Kept = MissingInside;\n",
                "  const wrong: string = 1;\n",
                "}\n",
            ),
            "MissingInside",
        ),
        (
            "class-container.ts",
            concat!(
                "class Holder {\n",
                "  method(value: ) {\n",
                "    type Kept = MissingMemberInside;\n",
                "    const wrongMember: string = 1;\n",
                "  }\n",
                "}\n",
            ),
            "MissingMemberInside",
        ),
    ];

    for (path, source, missing) in cases {
        let mut service = LanguageService::new(semantic_options());
        service.open(path, Arc::<str>::from(source));
        let result = service.semantic_diagnostics(path);
        assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(
            semantic_fingerprint(&result),
            vec![(
                path.to_string(),
                2304,
                source.find(missing).expect("independent required type") as u32,
                missing.len() as u32,
                DiagnosticCategory::Error,
                format!("Cannot find name '{missing}'."),
                Vec::new(),
            )],
            "{path}: {:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn nonclaimed_container_preserves_descendant_overload_adjacency() {
    let source = concat!(
        "function recoveredHost(value: ) {\n",
        "  function nested(value: string): string;\n",
        "  function nested(value: string) { return value; }\n",
        "  type Kept = MissingAfterOverload;\n",
        "}\n",
    );
    let path = "nested-overload.ts";
    let mut service = LanguageService::new(semantic_options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![(
            path.to_string(),
            2304,
            source
                .find("MissingAfterOverload")
                .expect("independent required type") as u32,
            "MissingAfterOverload".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingAfterOverload'.".to_string(),
            Vec::new(),
        )],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn recovered_parameter_initializer_keeps_nested_statement_siblings() {
    let source = concat!(
        "function outer(\n",
        "  callback = () => { type Kept = MissingInitializerInside; },\n",
        "  bad: \n",
        ") {}\n",
    );
    let path = "parameter-initializer.ts";
    let mut service = LanguageService::new(semantic_options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![(
            path.to_string(),
            2304,
            source
                .find("MissingInitializerInside")
                .expect("nested initializer sibling") as u32,
            "MissingInitializerInside".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingInitializerInside'.".to_string(),
            Vec::new(),
        )],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn recovered_parameter_host_withholds_dependent_body_relations() {
    let source = concat!(
        "function shell(value: number = ) {\n",
        "  const dependent: string = value;\n",
        "  type Kept = MissingParameterSibling;\n",
        "}\n",
    );
    let path = "parameter-dependency.ts";
    let mut service = LanguageService::new(semantic_options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![(
            path.to_string(),
            2304,
            source
                .find("MissingParameterSibling")
                .expect("independent body sibling") as u32,
            "MissingParameterSibling".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingParameterSibling'.".to_string(),
            Vec::new(),
        )],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn nonclaimed_switch_withholds_dependent_relations_but_keeps_name_leaves() {
    let source = concat!(
        "function known(value: string, other: string) {}\n",
        "function shell(value: string | number) {\n",
        "  let target: string = '';\n",
        "  switch (value.) {\n",
        "    default:\n",
        "      target = value;\n",
        "      MissingCall(value);\n",
        "      known(value, 1);\n",
        "      known(value, MissingArg);\n",
        "      const kept: MissingSwitchSibling = 1;\n",
        "  }\n",
        "}\n",
    );
    let path = "switch-dependency.ts";
    let mut service = LanguageService::new(semantic_options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        ["MissingCall", "MissingArg", "MissingSwitchSibling"]
            .into_iter()
            .map(|name| {
                (
                    path.to_string(),
                    2304,
                    source.find(name).expect("independent missing name") as u32,
                    name.len() as u32,
                    DiagnosticCategory::Error,
                    format!("Cannot find name '{name}'."),
                    Vec::new(),
                )
            })
            .collect::<Vec<_>>(),
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn valid_reference_conditions_do_not_create_capability_regions() {
    let compiler = Compiler::new();
    let cases = [
        ("direct-condition.ts", "value", "typeof value"),
        (
            "renamed-condition.ts",
            "candidate",
            "(((typeof candidate)))",
        ),
        ("wrapped-condition.ts", "subject", "typeof (((subject)))"),
    ];

    for (path, binder, condition) in cases {
        let source = format!(
            concat!(
                "function inspect({binder}: unknown) {{\n",
                "  if (({condition}) === \"object\") {{ {binder}; }}\n",
                "  const independent = 1;\n",
                "  const wrong: string = independent;\n",
                "}}\n",
            ),
            binder = binder,
            condition = condition,
        );
        let roots = vec![SourceInput::new(path, Arc::<str>::from(source.clone()))];
        let forward = compiler.compile(roots.clone(), &semantic_options());
        let repeated = compiler.compile(roots, &semantic_options());
        let expected = vec![(
            path.to_string(),
            2322,
            source.find("wrong").expect("independent relation") as u32,
            "wrong".len() as u32,
            DiagnosticCategory::Error,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            Vec::new(),
        )];

        for output in [&forward, &repeated] {
            assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
            assert_eq!(
                output.exit_status,
                CompileExitStatus::DiagnosticsPresentOutputsSkipped
            );
            assert_eq!(
                diagnostic_fingerprint(output),
                expected,
                "{path}: {:#?}",
                output.diagnostics,
            );
        }
        assert_eq!(forward.stats.types, repeated.stats.types);
    }
}

#[test]
fn nonclaimed_if_withholds_dependent_branch_relations() {
    let source = concat!(
        "declare function isString(value: string | number, mode: unknown): value is string;\n",
        "function shell(value: string | number) {\n",
        "  if (isString(value, `head${\"mode\"}tail`)) {\n",
        "    const dependent: string = value;\n",
        "    type Kept = MissingIfSibling;\n",
        "  }\n",
        "}\n",
    );
    let path = "if-dependency.ts";
    let mut service = LanguageService::new(semantic_options());
    service.open(path, Arc::<str>::from(source));

    let result = service.semantic_diagnostics(path);
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![(
            path.to_string(),
            2304,
            source
                .find("MissingIfSibling")
                .expect("independent branch sibling") as u32,
            "MissingIfSibling".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingIfSibling'.".to_string(),
            Vec::new(),
        )],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn valid_switch_preserves_prefix_and_suffix_diagnostics() {
    let source = concat!(
        "function consume(text: string) {}\n",
        "function shell(value: { tag: string }) {\n",
        "  const before: string = value;\n",
        "  switch (value.tag) {\n",
        "    default:\n",
        "      consume(value);\n",
        "  }\n",
        "  const after: string = value;\n",
        "  consume(value);\n",
        "}\n",
    );
    let path = "switch-locality.ts";
    let roots = vec![
        SourceInput::new(path, Arc::<str>::from(source)),
        SourceInput::new("stable.ts", Arc::<str>::from("const stable = 1;")),
    ];
    let mut reversed = roots.clone();
    reversed.reverse();
    let compiler = Compiler::new();
    let forward = compiler.compile(roots.clone(), &semantic_options());
    let repeated = compiler.compile(roots, &semantic_options());
    let reverse = compiler.compile(reversed, &semantic_options());

    let expected = vec![
        (
            path.to_string(),
            2322,
            source.find("before").expect("independent prefix relation") as u32,
            "before".len() as u32,
            DiagnosticCategory::Error,
            "Type '{ tag: string; }' is not assignable to type 'string'.".to_string(),
            Vec::new(),
        ),
        (
            path.to_string(),
            2345,
            source.find("consume(value)").expect("switch relation") as u32
                + "consume(".len() as u32,
            "value".len() as u32,
            DiagnosticCategory::Error,
            "Argument of type '{ tag: string; }' is not assignable to parameter of type 'string'."
                .to_string(),
            Vec::new(),
        ),
        (
            path.to_string(),
            2322,
            source.find("after").expect("independent suffix relation") as u32,
            "after".len() as u32,
            DiagnosticCategory::Error,
            "Type '{ tag: string; }' is not assignable to type 'string'.".to_string(),
            Vec::new(),
        ),
        (
            path.to_string(),
            2345,
            source.rfind("consume(value)").expect("suffix relation") as u32
                + "consume(".len() as u32,
            "value".len() as u32,
            DiagnosticCategory::Error,
            "Argument of type '{ tag: string; }' is not assignable to parameter of type 'string'."
                .to_string(),
            Vec::new(),
        ),
    ];
    for output in [&forward, &repeated, &reverse] {
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(
            diagnostic_fingerprint(output),
            expected,
            "{:#?}",
            output.diagnostics,
        );
    }
    assert_eq!(forward.stats.types, repeated.stats.types);
    assert_eq!(forward.stats.types, reverse.stats.types);
}

#[test]
fn required_type_claim_is_independent_from_semantic_check_claim() {
    let source = concat!(
        "function inspect(value: { tag: string }) {\n",
        "  switch (value.tag) { default: break; }\n",
        "  type Kept = MissingInside;\n",
        "  const wrong: string = 1;\n",
        "}\n",
    );
    let mut service = LanguageService::new(semantic_options());
    service.open("independent-targets.ts", Arc::<str>::from(source));

    let result = service.semantic_diagnostics("independent-targets.ts");
    assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&result),
        vec![
            (
                "independent-targets.ts".to_string(),
                2304,
                source.find("MissingInside").expect("required type") as u32,
                "MissingInside".len() as u32,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingInside'.".to_string(),
                Vec::new(),
            ),
            (
                "independent-targets.ts".to_string(),
                2322,
                source.find("wrong").expect("independent semantic relation") as u32,
                "wrong".len() as u32,
                DiagnosticCategory::Error,
                "Type 'number' is not assignable to type 'string'.".to_string(),
                Vec::new(),
            ),
        ],
        "{:#?}",
        result.diagnostics,
    );
}

#[test]
fn opaque_namespace_body_is_closed_without_poisoning_a_following_sibling() {
    for (path, host) in [
        ("namespace-container.ts", "Container"),
        ("namespace-vessel.ts", "Vessel"),
    ] {
        let source = format!(
            "const before: MissingBefore = 1;\n\
             namespace {host} {{\n\
               class Shape {{ value: string; }}\n\
               let current: Shape;\n\
               current = current;\n\
             }}\n\
             const after: MissingAfter = 1;\n"
        );
        let mut service = LanguageService::new(semantic_options());
        service.open(path, Arc::<str>::from(source.clone()));

        let result = service.semantic_diagnostics(path);
        assert_eq!(result.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(
            semantic_fingerprint(&result),
            ["MissingBefore", "MissingAfter"]
                .into_iter()
                .map(|name| (
                    path.to_string(),
                    2304,
                    source.find(name).expect("independent sibling") as u32,
                    name.len() as u32,
                    DiagnosticCategory::Error,
                    format!("Cannot find name '{name}'."),
                    Vec::new(),
                ))
                .collect::<Vec<_>>(),
            "{:#?}",
            result.diagnostics,
        );
    }
}

#[test]
fn assertion_declarator_lists_emit_the_authored_assertion_type() {
    for (affected, expected_js, expected_declaration, expected_completion) in [
        (
            "export const x = value as T, y = 1;",
            "export const x = value, y = 1;\n",
            "export declare const x: T, y = 1;\n",
            SemanticCompletion::Complete,
        ),
        (
            "export const renamed = <Renamed>value, changed = 1;",
            "export const renamed = value, changed = 1;\n",
            "export declare const renamed: Renamed, changed = 1;\n",
            SemanticCompletion::Deferred,
        ),
    ] {
        let output = Compiler::new().compile(
            vec![
                SourceInput::new("affected.ts", Arc::<str>::from(affected)),
                SourceInput::new("stable.ts", Arc::<str>::from("export const stable = 1;")),
            ],
            &CompilerOptions {
                target: "es2022".to_string(),
                declaration: true,
                no_check: true,
                no_emit_on_error: false,
                ..CompilerOptions::default()
            },
        );
        let mut paths = output
            .emitted_files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        paths.sort();
        assert_eq!(
            paths,
            ["affected.d.ts", "affected.js", "stable.d.ts", "stable.js"],
            "{affected}",
        );
        let emitted = |path: &str| {
            output
                .emitted_files
                .iter()
                .find(|file| file.path.to_str() == Some(path))
                .map(|file| file.text.as_str())
        };
        assert_eq!(emitted("affected.js"), Some(expected_js), "{affected}");
        assert_eq!(
            emitted("affected.d.ts"),
            Some(expected_declaration),
            "{affected}",
        );
        assert!(output.diagnostics.is_empty(), "{affected}: {output:#?}");
        assert_eq!(
            output.semantic_completion, expected_completion,
            "{affected}"
        );
        assert_eq!(
            output.exit_status,
            if expected_completion.is_complete() {
                CompileExitStatus::Success
            } else {
                CompileExitStatus::SemanticIncomplete
            },
            "{affected}",
        );
    }

    let source = concat!(
        "const x = value as T changed\n",
        "const y = 1;\n",
        "y;\n",
        "const independent: MissingIndependent = 1;",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "bounded-tail.ts",
            Arc::<str>::from(source),
        )],
        &semantic_options(),
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostic_fingerprint(&output),
        vec![(
            "bounded-tail.ts".to_string(),
            2304,
            source.find("MissingIndependent").unwrap() as u32,
            "MissingIndependent".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingIndependent'.".to_string(),
            Vec::new(),
        )],
    );
}

#[test]
fn opaque_object_members_and_loops_withhold_only_their_file_products() {
    for affected in [
        "export const value = { get renamed() { return 1; } };",
        "for (const { renamed } of values) { renamed; } export const value = 1;",
        "export const value = class Renamed {};",
    ] {
        let output = Compiler::new().compile(
            vec![
                SourceInput::new("affected.ts", Arc::<str>::from(affected)),
                SourceInput::new("stable.ts", Arc::<str>::from("export const stable = 1;")),
            ],
            &CompilerOptions {
                target: "es2022".to_string(),
                declaration: true,
                no_check: true,
                no_emit_on_error: false,
                ..CompilerOptions::default()
            },
        );
        let mut paths = output
            .emitted_files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        paths.sort();
        assert_eq!(paths, ["stable.d.ts", "stable.js"], "{affected}");
        assert!(output.diagnostics.is_empty(), "{affected}: {output:#?}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}
