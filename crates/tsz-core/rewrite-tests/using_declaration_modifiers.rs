use std::path::PathBuf;
use std::sync::Arc;

use tsz::diagnostics::{Diagnostic, DiagnosticCategory};
use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::parse_source;
use tsz::{CompilerOptions, SemanticCompletion};

fn source(text: &str) -> SourceText {
    SourceText::new(
        FileId(0),
        PathBuf::from("using-modifier.ts"),
        Arc::<str>::from(text),
    )
}

fn rows(diagnostics: &[Diagnostic]) -> Vec<(u32, u32, u32, DiagnosticCategory, &str)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
            )
        })
        .collect()
}

#[test]
fn parser_owns_exact_restricted_using_modifier_diagnostics() {
    for (source_text, code, length, message) in [
        (
            "export using renamed = null;",
            1491,
            6,
            "'export' modifier cannot appear on a 'using' declaration.",
        ),
        (
            "declare using renamed = null;",
            1491,
            7,
            "'declare' modifier cannot appear on a 'using' declaration.",
        ),
        (
            "export await using renamed = null;",
            1495,
            6,
            "'export' modifier cannot appear on an 'await using' declaration.",
        ),
        (
            "declare await using renamed = null;",
            1495,
            7,
            "'declare' modifier cannot appear on an 'await using' declaration.",
        ),
    ] {
        let parsed = parse_source(&source(source_text));
        assert_eq!(
            rows(&parsed.diagnostics),
            [(code, 0, length, DiagnosticCategory::Error, message)],
            "{source_text}: {:#?}",
            parsed.diagnostics,
        );
    }
}

#[test]
fn service_claims_the_parser_result_and_keeps_valid_using_forms_clean() {
    let mut service = LanguageService::new(CompilerOptions {
        no_emit: true,
        target: "esnext".to_string(),
        ..CompilerOptions::default()
    });
    for (path, source_text, expected) in [
        (
            "export-using.ts",
            "export using resource = null;",
            Some((
                1491,
                6,
                "'export' modifier cannot appear on a 'using' declaration.",
            )),
        ),
        (
            "export-await-using.ts",
            "export await using asyncResource = null;",
            Some((
                1495,
                6,
                "'export' modifier cannot appear on an 'await using' declaration.",
            )),
        ),
        ("valid-using.ts", "using local = null;", None),
        (
            "valid-await-using.ts",
            "await using asyncLocal = null;",
            None,
        ),
    ] {
        service.open(path, Arc::<str>::from(source_text));
        let result = service.syntactic_diagnostics(path);
        assert_eq!(
            result.syntactic_completion,
            SemanticCompletion::Complete,
            "{path}: {:#?}",
            result.diagnostics,
        );
        let expected_rows = expected.map_or_else(Vec::new, |(code, length, message)| {
            vec![(code, 0, length, DiagnosticCategory::Error, message)]
        });
        assert_eq!(rows(&result.diagnostics), expected_rows, "{path}");
    }
}
