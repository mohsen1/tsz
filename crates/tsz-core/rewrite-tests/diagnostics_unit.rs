use super::{Diagnostic, RelatedInformation, sort_and_deduplicate};
use crate::source::{FileId, SourceText, Span};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn external_config_diagnostics_publish_utf16_coordinates() {
    let source = Arc::<str>::from("\u{feff}// café 😀\n{\"target\":\"oops\"}");
    let byte_start = source.find("oops").expect("value") as u32;
    let diagnostic = Diagnostic::error_at_text(
        "tsconfig.json".to_string(),
        byte_start,
        4,
        source,
        "Invalid value.".to_string(),
        6046,
    );
    assert_eq!((diagnostic.start, diagnostic.length), (23, 4));
    assert_eq!(
        diagnostic.render(None),
        "tsconfig.json(2,12): error TS6046: Invalid value."
    );
}

#[test]
fn source_diagnostic_rendering_uses_typescript_line_terminators() {
    for (separator, expected_start) in [
        ("\n", 17),
        ("\r\n", 18),
        ("\r", 17),
        ("\u{2028}", 17),
        ("\u{2029}", 17),
    ] {
        let text = Arc::<str>::from(format!("// ≤{separator}var x = /\\u{{110000}}/gu;"));
        let source = SourceText::new(FileId(4), PathBuf::from("lines.ts"), Arc::clone(&text));
        let start = text.find("110000").expect("digits");
        let diagnostic = Diagnostic::at(
            &source,
            Span::new(FileId(4), start, start + 6),
            "Out of range.".to_string(),
            1198,
        );
        assert_eq!(
            (diagnostic.start, diagnostic.length),
            (expected_start, 6),
            "{separator:?}",
        );
        assert_eq!(
            diagnostic.render(Some(&source)),
            "lines.ts(2,13): error TS1198: Out of range.",
            "{separator:?}",
        );
    }
}

#[test]
fn diagnostic_order_uses_full_public_identity_before_internal_file_id() {
    let related = RelatedInformation {
        file: "origin.ts".to_string(),
        start: 4,
        length: 2,
        message_text: "Origin.".to_string(),
        code: 9001,
        depth: 1,
    };
    let mut duplicate = Diagnostic::error("case.ts".to_string(), 3, 2, "B".to_string(), 2000)
        .with_related_information(vec![related.clone()]);
    duplicate.file_id = Some(FileId(1));
    let mut duplicate_from_another_host = duplicate.clone();
    duplicate_from_another_host.file_id = Some(FileId(3));
    let mut distinct_related =
        Diagnostic::error("case.ts".to_string(), 3, 2, "B".to_string(), 2000)
            .with_related_information(vec![RelatedInformation::unlocated("Other.", 9002, 1)]);
    distinct_related.file_id = Some(FileId(2));
    let mut diagnostics = vec![
        duplicate.clone(),
        Diagnostic::error("case.ts".to_string(), 3, 1, "C".to_string(), 3000),
        Diagnostic::error("case.ts".to_string(), 3, 2, "A".to_string(), 1000)
            .with_related_information(vec![related]),
        distinct_related,
        duplicate_from_another_host,
    ];

    sort_and_deduplicate(&mut diagnostics);

    assert_eq!(diagnostics.len(), 4);
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.length,
                diagnostic.code,
                diagnostic.message_text.as_str(),
                diagnostic
                    .related_information
                    .first()
                    .map_or("", |related| related.message_text.as_str()),
            ))
            .collect::<Vec<_>>(),
        [
            (1, 3000, "C", ""),
            (2, 1000, "A", "Origin."),
            (2, 2000, "B", "Other."),
            (2, 2000, "B", "Origin."),
        ]
    );
}
