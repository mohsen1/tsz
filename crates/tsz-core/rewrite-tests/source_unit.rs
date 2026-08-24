use super::{FileId, SourceText, Span};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn source_coordinates_translate_bytes_to_absolute_utf16() {
    let text = Arc::<str>::from("// café 😀\r\nvar value = /x/z;");
    let source = SourceText::new(FileId(1), PathBuf::from("case.ts"), Arc::clone(&text));
    let start = text.find("z;").expect("flag") as u32;
    assert_eq!(
        source.utf16_span(Span::new(FileId(1), start as usize, start as usize + 1)),
        (27, 1)
    );
    assert_eq!(source.line_and_column(27), (2, 16));
}

#[test]
fn decoded_bom_is_an_ordinary_utf16_source_unit() {
    let text = Arc::<str>::from("\u{feff}var value = /x/z;");
    let source = SourceText::new(FileId(2), PathBuf::from("bom.ts"), Arc::clone(&text));
    let start = text.find("z;").expect("flag") as u32;
    assert_eq!(
        source.utf16_span(Span::new(FileId(2), start as usize, start as usize + 1)),
        (16, 1)
    );
    assert_eq!(source.line_and_column(16), (1, 17));
}

#[test]
fn every_typescript_line_terminator_starts_a_new_utf16_line() {
    for (separator, expected_start) in [
        ("\n", 17),
        ("\r\n", 18),
        ("\r", 17),
        ("\u{2028}", 17),
        ("\u{2029}", 17),
    ] {
        let text = Arc::<str>::from(format!("// ≤{separator}var x = /\\u{{110000}}/gu;"));
        let source = SourceText::new(FileId(3), PathBuf::from("lines.ts"), Arc::clone(&text));
        let start = text.find("110000").expect("digits");
        assert_eq!(
            source.utf16_span(Span::new(FileId(3), start, start + 6)),
            (expected_start, 6),
            "{separator:?}",
        );
        assert_eq!(
            source.line_and_column(expected_start),
            (2, 13),
            "{separator:?}",
        );
    }
}
