use super::*;
use tsz::diagnostics::RelatedInformation;

pub(super) fn roots() -> Vec<SourceInput> {
    vec![
        SourceInput::new("gap.ts", Arc::<str>::from("const local = `plain`;")),
        SourceInput::new(
            "sibling.ts",
            Arc::<str>::from("const sibling: string = missingOwned;"),
        ),
    ]
}

pub(super) fn roots_with_cross_file_demand() -> Vec<SourceInput> {
    vec![
        SourceInput::new(
            "gap.ts",
            Arc::<str>::from("const shared: string = 1; const local = `plain`;"),
        ),
        SourceInput::new(
            "sibling.ts",
            Arc::<str>::from("const copy: string = shared; const sibling: string = missingOwned;"),
        ),
    ]
}

pub(super) fn roots_with_partially_nonclaimed_global_group() -> Vec<SourceInput> {
    vec![
        SourceInput::new(
            "declared.ts",
            Arc::<str>::from("function shared(value: string): string;"),
        ),
        SourceInput::new(
            "gap.ts",
            Arc::<str>::from("function shared(value: string) { return `plain`; }"),
        ),
        SourceInput::new(
            "consumer.ts",
            Arc::<str>::from("const value = shared(1); const kept: MissingOwned = 1;"),
        ),
    ]
}

pub(super) type DiagnosticFingerprint = (
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

pub(super) fn diagnostic_fingerprint(output: &tsz::CompileOutput) -> Vec<DiagnosticFingerprint> {
    diagnostics_fingerprint(&output.diagnostics)
}

pub(super) fn diagnostics_fingerprint(
    diagnostics: &[tsz::diagnostics::Diagnostic],
) -> Vec<DiagnosticFingerprint> {
    diagnostics
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

pub(super) fn emitted_fingerprint(output: &tsz::CompileOutput) -> Vec<(String, String, bool)> {
    output
        .emitted_files
        .iter()
        .map(|file| {
            (
                file.path.to_string_lossy().into_owned(),
                file.text.clone(),
                file.declaration,
            )
        })
        .collect()
}

pub(super) fn semantic_fingerprint(
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

pub(super) fn semantic_options() -> CompilerOptions {
    CompilerOptions {
        target: "es2015".to_string(),
        no_emit: true,
        ..CompilerOptions::default()
    }
}

pub(super) fn assert_named_sibling_survives(source: &str) {
    let options = CompilerOptions::default();
    let mut service = LanguageService::new(options.clone());
    service.open("mixed.ts", Arc::<str>::from(source.to_string()));
    let syntactic = service.syntactic_diagnostics("mixed.ts");
    assert_eq!(syntactic.syntactic_completion, SemanticCompletion::Complete);
    let semantic = service.semantic_diagnostics("mixed.ts");
    assert_eq!(semantic.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(
        semantic_fingerprint(&semantic),
        vec![(
            "mixed.ts".to_string(),
            2304,
            source.find("MissingOwned").expect("missing name") as u32,
            "MissingOwned".len() as u32,
            DiagnosticCategory::Error,
            "Cannot find name 'MissingOwned'.".to_string(),
            Vec::new(),
        )],
    );

    let output = service.compile();
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(
        output.stats.types > 0,
        "the claimed sibling must be checked"
    );
    let expected_compiler_product = if syntactic.diagnostics.is_empty() {
        &semantic.diagnostics
    } else {
        &syntactic.diagnostics
    };
    assert_eq!(output.diagnostics, *expected_compiler_product);

    let mut no_check_service = LanguageService::new(CompilerOptions {
        no_check: true,
        ..options
    });
    no_check_service.open("mixed.ts", Arc::<str>::from(source.to_string()));
    let no_check_syntactic = no_check_service.syntactic_diagnostics("mixed.ts");
    assert_eq!(
        no_check_syntactic.syntactic_completion,
        SemanticCompletion::Complete
    );
    let no_check_semantic = no_check_service.semantic_diagnostics("mixed.ts");
    assert_eq!(
        no_check_semantic.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(no_check_semantic.diagnostics.is_empty());
    let no_check = no_check_service.compile();
    assert_eq!(no_check.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(no_check.exit_status, CompileExitStatus::SemanticIncomplete);
    assert_eq!(no_check.stats.types, 0);
    assert_eq!(no_check.diagnostics, no_check_syntactic.diagnostics);
}
