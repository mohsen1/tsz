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

#[test]
fn revision_coordinates_round_trip_mixed_terminators_and_astral_columns() {
    let text = Arc::<str>::from("a\r\nb\rc\nd\u{2028}e\u{2029}😀f");
    let source = SourceText::new(FileId(4), PathBuf::from("mixed.ts"), Arc::clone(&text));

    for (byte, position) in [
        (0, (1, 1)),
        (1, (1, 2)),
        (2, (1, 3)),
        (3, (2, 1)),
        (4, (2, 2)),
        (5, (3, 1)),
        (6, (3, 2)),
        (7, (4, 1)),
        (8, (4, 2)),
        (11, (5, 1)),
        (12, (5, 2)),
        (15, (6, 1)),
        (19, (6, 3)),
        (20, (6, 4)),
    ] {
        assert_eq!(source.position(byte), Some(position));
        assert_eq!(source.byte_offset(position.0, position.1), Some(byte));
    }
    assert_eq!(source.utf16_range(15, 4), Some((11, 2)));
    assert_eq!(source.text(), text.as_ref());
}

#[test]
fn revision_coordinates_reject_invalid_protocol_positions_and_byte_offsets() {
    let text = Arc::<str>::from("a\r\n😀z");
    let source = SourceText::new(FileId(5), PathBuf::from("invalid.ts"), text);

    for (line, column) in [(0, 1), (1, 0), (1, 4), (2, 2), (3, 1)] {
        assert_eq!(source.byte_offset(line, column), None);
    }
    assert_eq!(source.position(4), None, "byte inside astral scalar");
    assert_eq!(source.position(2), Some((1, 3)), "byte at LF in CRLF");
    assert_eq!(source.position(9), None, "byte beyond source");
    assert_eq!(source.utf16_range(3, 1), None);
    assert_eq!(source.utf16_range(u32::MAX, 2), None);

    let trailing = SourceText::new(FileId(6), PathBuf::from("trailing.ts"), Arc::from("a\n"));
    assert_eq!(trailing.byte_offset(2, 1), Some(2));
    assert_eq!(trailing.position(2), Some((2, 1)));
    assert_eq!(trailing.byte_offset(1, 3), None, "next-line alias");
}

#[test]
fn declaration_source_paths_match_case_sensitive_cross_platform_basenames() {
    for path in [
        "value.d.ts",
        "value.d.mts",
        "value.d.cts",
        "value.d.html.ts",
        "value.d.ts/",
        r"value.d.mts\",
        "value.d.html.ts/",
    ] {
        assert!(super::is_declaration_source_path(
            PathBuf::from(path).as_path()
        ));
    }
    for path in [
        "value.D.ts",
        "value.d.html.TS",
        "value.d.html.mts",
        "value.d.html.cts",
        "value.d.html.tsx",
        "value.d.ts//",
        "value.d.html.ts//",
        r"folder.d.parts\ordinary.ts",
        "http://server.d.ts",
        "file://server.d.html.ts/",
        "//server.d.ts",
        r"\\server.d.html.ts\",
    ] {
        assert!(!super::is_declaration_source_path(
            PathBuf::from(path).as_path()
        ));
    }
}
