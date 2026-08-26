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
    output
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
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "mixed.ts",
            Arc::<str>::from(source.to_string()),
        )],
        &options,
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(
        output.stats.types > 0,
        "the claimed sibling must be checked"
    );
    let missing_start = source.find("MissingOwned").expect("missing name") as u32;
    let missing = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2304)
        .collect::<Vec<_>>();
    assert_eq!(missing.len(), 1, "{:#?}", output.diagnostics);
    let missing = missing[0];
    assert_eq!(missing.start, missing_start);
    assert_eq!(missing.file, "mixed.ts");
    assert_eq!(missing.length, "MissingOwned".len() as u32);
    assert_eq!(missing.category, DiagnosticCategory::Error);
    assert_eq!(missing.message_text, "Cannot find name 'MissingOwned'.");
    assert!(missing.related_information.is_empty());

    let no_check = Compiler::new().compile(
        vec![SourceInput::new(
            "mixed.ts",
            Arc::<str>::from(source.to_string()),
        )],
        &CompilerOptions {
            no_check: true,
            ..options
        },
    );
    assert_eq!(no_check.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(no_check.stats.types, 0);
    assert!(
        no_check
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2304)
    );
}
