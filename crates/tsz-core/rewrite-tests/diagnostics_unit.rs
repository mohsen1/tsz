use super::Diagnostic;
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
