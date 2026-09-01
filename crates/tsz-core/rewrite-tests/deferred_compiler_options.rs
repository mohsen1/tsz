use std::collections::BTreeMap;

use tsz::config::{ProjectRequest, ProjectSelection, resolve_project};
use tsz::host::SystemHost;
use tsz::service::{LanguageService, ServiceQuery};
use tsz::{
    CompileExitStatus, Compiler, CompilerOptions, DeferredCompilerOption,
    DeferredCompilerOptionValue, SemanticCompletion, SourceInput,
};

fn options(option: DeferredCompilerOption, value: DeferredCompilerOptionValue) -> CompilerOptions {
    CompilerOptions {
        deferred_options: BTreeMap::from([(option, value)]),
        ..CompilerOptions::default()
    }
}

fn ordered_inputs(reverse: bool) -> Vec<SourceInput> {
    let mut inputs = vec![
        SourceInput::new("zeta.ts", "export const zeta = 1;\n"),
        SourceInput::new("alpha.ts", "export const alpha = 2;\n"),
    ];
    if reverse {
        inputs.reverse();
    }
    inputs
}

#[test]
fn default_equivalent_boolean_false_is_inert_for_default_false_options() {
    let compiler = Compiler::new();
    let baseline_options = CompilerOptions {
        declaration: true,
        ..CompilerOptions::default()
    };
    let baseline = compiler.compile(ordered_inputs(false), &baseline_options);
    assert_eq!(baseline.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(baseline.exit_status, CompileExitStatus::Success);
    assert!(baseline.diagnostics.is_empty());
    let expected_products = baseline
        .emitted_files
        .iter()
        .map(|file| (file.path.clone(), file.text.clone(), file.declaration))
        .collect::<Vec<_>>();
    assert_eq!(
        expected_products
            .iter()
            .filter(|(_, _, declaration)| *declaration)
            .count(),
        2
    );
    assert_eq!(
        expected_products
            .iter()
            .filter(|(_, _, declaration)| !*declaration)
            .count(),
        2
    );

    for option in [
        DeferredCompilerOption::AlwaysStrict,
        DeferredCompilerOption::DownlevelIteration,
        DeferredCompilerOption::NoEmitHelpers,
        DeferredCompilerOption::ImportHelpers,
        DeferredCompilerOption::EsModuleInterop,
        DeferredCompilerOption::ExperimentalDecorators,
        DeferredCompilerOption::EmitDecoratorMetadata,
        DeferredCompilerOption::ExactOptionalPropertyTypes,
        DeferredCompilerOption::PreserveConstEnums,
        DeferredCompilerOption::VerbatimModuleSyntax,
        DeferredCompilerOption::RewriteRelativeImportExtensions,
        DeferredCompilerOption::IsolatedModules,
        DeferredCompilerOption::StripInternal,
    ] {
        let mut false_options = options(option, DeferredCompilerOptionValue::Boolean(false));
        false_options.declaration = true;
        for reverse in [false, true] {
            for repetition in 0..2 {
                let output = compiler.compile(ordered_inputs(reverse), &false_options);
                assert_eq!(
                    output.semantic_completion,
                    SemanticCompletion::Complete,
                    "{option:?}, reverse={reverse}, repetition={repetition}"
                );
                assert_eq!(
                    output.exit_status,
                    CompileExitStatus::Success,
                    "{option:?}, reverse={reverse}, repetition={repetition}"
                );
                assert!(
                    output.diagnostics.is_empty(),
                    "{option:?}, reverse={reverse}, repetition={repetition}: {:#?}",
                    output.diagnostics
                );
                let products = output
                    .emitted_files
                    .iter()
                    .map(|file| (file.path.clone(), file.text.clone(), file.declaration))
                    .collect::<Vec<_>>();
                assert_eq!(
                    products, expected_products,
                    "{option:?}, reverse={reverse}, repetition={repetition}"
                );
            }
        }
    }
}

#[test]
fn strict_dependent_values_close_semantics_in_same_and_cross_file_products() {
    let compiler = Compiler::new();
    let sources = [
        ("alpha.ts", "export const alpha = 1; alpha;"),
        ("zeta.ts", "export const zeta = 2; zeta;"),
    ];

    // TypeScript 7 lets an explicit strict-family suboption override the
    // umbrella `strict` value. Until each suboption has a semantic owner, an
    // authored value that changes or selects that behavior cannot be claimed.
    for option in [
        DeferredCompilerOption::NoImplicitThis,
        DeferredCompilerOption::StrictBindCallApply,
        DeferredCompilerOption::StrictFunctionTypes,
        DeferredCompilerOption::UseUnknownInCatchVariables,
    ] {
        for strict in [false, true] {
            for authored_value in [None, Some(false), Some(true)] {
                let mut compiler_options = CompilerOptions {
                    strict,
                    declaration: true,
                    ..CompilerOptions::default()
                };
                if let Some(value) = authored_value {
                    compiler_options
                        .deferred_options
                        .insert(option, DeferredCompilerOptionValue::Boolean(value));
                }
                let expected_nonclaim =
                    matches!(authored_value, Some(true)) || strict && authored_value == Some(false);
                let expected_completion = if expected_nonclaim {
                    SemanticCompletion::Deferred
                } else {
                    SemanticCompletion::Complete
                };
                let expected_exit = if expected_nonclaim {
                    CompileExitStatus::SemanticIncomplete
                } else {
                    CompileExitStatus::Success
                };

                for reverse in [false, true] {
                    let output = compiler.compile(ordered_inputs(reverse), &compiler_options);
                    assert_eq!(
                        output.semantic_completion, expected_completion,
                        "{option:?}, strict={strict}, authored={authored_value:?}, reverse={reverse}",
                    );
                    assert_eq!(
                        output.exit_status, expected_exit,
                        "{option:?}, strict={strict}, authored={authored_value:?}, reverse={reverse}",
                    );
                    assert!(output.diagnostics.is_empty());
                    assert_eq!(
                        output
                            .emitted_files
                            .iter()
                            .filter(|file| !file.declaration)
                            .count(),
                        2,
                        "JavaScript remains independent",
                    );
                    assert_eq!(
                        output
                            .emitted_files
                            .iter()
                            .filter(|file| file.declaration)
                            .count(),
                        usize::from(!expected_nonclaim) * 2,
                        "declaration emit follows semantic-type completion",
                    );
                }

                let mut service = LanguageService::new(compiler_options);
                for (path, source) in sources {
                    service.open(path, source);
                }
                for (path, source) in sources {
                    assert_eq!(
                        service.semantic_diagnostics(path).semantic_completion,
                        expected_completion,
                        "{option:?}, strict={strict}, authored={authored_value:?}, {path}",
                    );
                    assert_eq!(
                        service.syntactic_diagnostics(path).syntactic_completion,
                        SemanticCompletion::Complete,
                        "syntax remains independent in {path}",
                    );
                    let offset = source.rfind(';').expect("trailing reference") as u32 - 1;
                    assert!(matches!(
                        service.definition_and_bound_span(path, offset),
                        ServiceQuery::Claimed(Some(_))
                    ));
                }
            }
        }
    }
}

#[test]
fn swallowed_template_identity_closes_only_exhaustive_navigation_publicly() {
    let sources = [
        (
            "tagged.ts",
            "declare const tag: any; const renamed = 1; tag`${renamed}`; renamed;",
        ),
        ("independent.ts", "const independent = 2; independent;"),
    ];
    let mut service = LanguageService::new(CompilerOptions::default());
    for (path, source) in sources {
        service.open(path, source);
    }
    let files = sources.map(|(path, _)| path.to_string());

    for (path, source) in sources {
        let offset = source.rfind(';').expect("trailing reference") as u32 - 1;
        assert!(matches!(
            service.references(path, offset),
            ServiceQuery::Nonclaimed(_)
        ));
        assert!(matches!(
            service.document_highlights(path, offset, &files),
            ServiceQuery::Nonclaimed(_)
        ));
        assert!(matches!(
            service.rename(path, offset),
            ServiceQuery::Nonclaimed(_)
        ));
        assert!(matches!(
            service.quick_info(path, offset),
            ServiceQuery::Claimed(Some(_))
        ));
        assert!(matches!(
            service.definition_and_bound_span(path, offset),
            ServiceQuery::Claimed(Some(_))
        ));
    }
}

#[test]
fn active_boolean_values_withhold_only_their_owned_products() {
    let compiler = Compiler::new();
    let source = || SourceInput::new("case.ts", "export const value = 1;\n");

    let mut javascript_option = options(
        DeferredCompilerOption::NoEmitHelpers,
        DeferredCompilerOptionValue::Boolean(true),
    );
    javascript_option.declaration = true;
    let javascript_nonclaim = compiler.compile(vec![source()], &javascript_option);
    assert_eq!(
        javascript_nonclaim.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        javascript_nonclaim.exit_status,
        CompileExitStatus::SemanticIncomplete
    );
    assert_eq!(javascript_nonclaim.emitted_files.len(), 1);
    assert!(javascript_nonclaim.emitted_files[0].declaration);

    let mut declaration_option = options(
        DeferredCompilerOption::StripInternal,
        DeferredCompilerOptionValue::Boolean(true),
    );
    declaration_option.declaration = true;
    let declaration_nonclaim = compiler.compile(vec![source()], &declaration_option);
    assert_eq!(
        declaration_nonclaim.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        declaration_nonclaim.exit_status,
        CompileExitStatus::SemanticIncomplete
    );
    assert_eq!(declaration_nonclaim.emitted_files.len(), 1);
    assert!(!declaration_nonclaim.emitted_files[0].declaration);
}

#[test]
fn deferred_options_withhold_only_the_products_they_can_change() {
    let compiler = Compiler::new();
    let source = || SourceInput::new("case.ts", "export const value = 1;\n");

    let mut no_emit_helpers = options(
        DeferredCompilerOption::NoEmitHelpers,
        DeferredCompilerOptionValue::Boolean(true),
    );
    no_emit_helpers.no_emit = true;
    let diagnostic_only = compiler.compile(vec![source()], &no_emit_helpers);
    assert_eq!(
        diagnostic_only.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(diagnostic_only.exit_status, CompileExitStatus::Success);

    let strip_internal = options(
        DeferredCompilerOption::StripInternal,
        DeferredCompilerOptionValue::Boolean(true),
    );
    let javascript_only = compiler.compile(vec![source()], &strip_internal);
    assert_eq!(
        javascript_only.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(javascript_only.emitted_files.len(), 1);
    assert!(!javascript_only.emitted_files[0].declaration);

    let out_file = options(
        DeferredCompilerOption::OutFile,
        DeferredCompilerOptionValue::Path("bundle.js".into()),
    );
    let bundled = compiler.compile(vec![source()], &out_file);
    assert_eq!(bundled.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(bundled.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(bundled.emitted_files.is_empty());

    let semantic_strictness = options(
        DeferredCompilerOption::StrictFunctionTypes,
        DeferredCompilerOptionValue::Boolean(true),
    );
    let semantic_nonclaim = compiler.compile(vec![source()], &semantic_strictness);
    assert_eq!(
        semantic_nonclaim.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        semantic_nonclaim.exit_status,
        CompileExitStatus::SemanticIncomplete
    );
    assert_eq!(semantic_nonclaim.emitted_files.len(), 1);
    assert!(!semantic_nonclaim.emitted_files[0].declaration);
}

#[test]
fn program_and_source_kind_options_are_scoped_before_publication() {
    let compiler = Compiler::new();

    let mut module_resolution = options(
        DeferredCompilerOption::ModuleResolution,
        DeferredCompilerOptionValue::String("node16".to_string()),
    );
    module_resolution.no_emit = true;
    let program_wide = compiler.compile(
        vec![SourceInput::new("case.ts", "const value = 1;\n")],
        &module_resolution,
    );
    assert_eq!(
        program_wide.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        program_wide.exit_status,
        CompileExitStatus::SemanticIncomplete
    );

    let mut jsx = options(
        DeferredCompilerOption::Jsx,
        DeferredCompilerOptionValue::String("react-jsx".to_string()),
    );
    jsx.no_emit = true;
    let ordinary = compiler.compile(
        vec![SourceInput::new("ordinary.ts", "const value = 1;\n")],
        &jsx,
    );
    assert_eq!(ordinary.semantic_completion, SemanticCompletion::Complete);

    let ordinary_jsx_source = compiler.compile(
        vec![SourceInput::new("component.tsx", "const value = 1;\n")],
        &jsx,
    );
    assert_eq!(
        ordinary_jsx_source.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(ordinary_jsx_source.exit_status, CompileExitStatus::Success);

    let authored_jsx = compiler.compile(
        vec![SourceInput::new(
            "component.tsx",
            "const element = <section />;\n",
        )],
        &jsx,
    );
    assert_eq!(
        authored_jsx.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        authored_jsx.exit_status,
        CompileExitStatus::SemanticIncomplete
    );
}

#[test]
fn skip_lib_check_skips_declaration_diagnostics_without_nonclaiming_the_program() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("types.d.ts"),
        "declare const broken: MissingDeclarationType;\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("case.ts"),
        "const value: number = 42;\n",
    )
    .unwrap();
    let compile = |skip_lib_check| {
        std::fs::write(
            project.path().join("tsconfig.json"),
            format!(
                r#"{{"compilerOptions":{{"noEmit":true,"skipLibCheck":{skip_lib_check}}},"files":["types.d.ts","case.ts"]}}"#
            ),
        )
        .unwrap();
        let host = SystemHost::new(project.path());
        let resolved = resolve_project(
            &host,
            &ProjectRequest::new(ProjectSelection::Project(project.path().to_path_buf())),
        );
        let options = resolved.options.clone();
        Compiler::new().compile_resolved(resolved, &options)
    };

    let skipped = compile(true);
    assert!(skipped.diagnostics.is_empty(), "{:?}", skipped.diagnostics);
    assert_eq!(skipped.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(skipped.exit_status, CompileExitStatus::Success);

    let checked = compile(false);
    assert_eq!(
        checked
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2304]
    );
    assert_eq!(checked.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        checked.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn config_inheritance_preserves_authored_deferred_values() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("base.json"),
        r#"{
            "compilerOptions": {
                "alwaysStrict": true,
                "moduleResolution": "node16"
            }
        }"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("tsconfig.json"),
        r#"{
            "extends": "./base.json",
            "compilerOptions": { "alwaysStrict": false, "noEmit": true },
            "files": ["case.ts"]
        }"#,
    )
    .unwrap();
    std::fs::write(project.path().join("case.ts"), "const value = 1;\n").unwrap();

    let host = SystemHost::new(project.path().to_path_buf());
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(project.path().to_path_buf())),
    );
    assert_eq!(
        resolved
            .options
            .deferred_options
            .get(&DeferredCompilerOption::AlwaysStrict),
        Some(&DeferredCompilerOptionValue::Boolean(false))
    );
    assert_eq!(
        resolved
            .options
            .deferred_options
            .get(&DeferredCompilerOption::ModuleResolution),
        Some(&DeferredCompilerOptionValue::String("node16".to_string()))
    );
}

#[test]
fn inherited_false_override_is_capability_inert_but_reports_removed_option() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("base.json"),
        r#"{
            "compilerOptions": { "alwaysStrict": true }
        }"#,
    )
    .unwrap();
    let child_config = r#"{
            "extends": "./base.json",
            "compilerOptions": { "alwaysStrict": false, "noEmit": true },
            "files": ["case.ts"]
        }"#;
    std::fs::write(project.path().join("tsconfig.json"), child_config).unwrap();
    std::fs::write(project.path().join("case.ts"), "const value = 1;\n").unwrap();

    let host = SystemHost::new(project.path().to_path_buf());
    for repetition in 0..2 {
        let resolved = resolve_project(
            &host,
            &ProjectRequest::new(ProjectSelection::Project(project.path().to_path_buf())),
        );
        assert_eq!(
            resolved
                .options
                .deferred_options
                .get(&DeferredCompilerOption::AlwaysStrict),
            Some(&DeferredCompilerOptionValue::Boolean(false))
        );
        let options = resolved.options.clone();
        let output = Compiler::new().compile_resolved(resolved, &options);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "repetition={repetition}"
        );
        assert_eq!(
            output.exit_status,
            CompileExitStatus::DiagnosticsPresentOutputsSkipped,
            "repetition={repetition}"
        );
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!(
                "expected exactly TS5108, repetition={repetition}: {:#?}",
                output.diagnostics
            )
        };
        assert_eq!(
            (
                diagnostic.file.as_str(),
                diagnostic.start,
                diagnostic.length,
                diagnostic.code,
                diagnostic.message_text.as_str(),
            ),
            (
                "tsconfig.json",
                child_config.find("false").unwrap() as u32,
                "false".len() as u32,
                5108,
                "Option 'alwaysStrict=false' has been removed. Please remove it from your configuration.",
            ),
            "repetition={repetition}"
        );
        assert!(
            output.emitted_files.is_empty(),
            "noEmit must remain honored"
        );
    }
}
