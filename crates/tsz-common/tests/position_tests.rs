use super::*;

#[test]
fn test_line_map_simple() {
    let source = "line1\nline2\nline3";
    let map = LineMap::build(source);

    assert_eq!(map.line_count(), 3);

    // First character of first line
    assert_eq!(map.offset_to_position(0, source), Position::new(0, 0));
    // Last character of first line
    assert_eq!(map.offset_to_position(4, source), Position::new(0, 4));
    // First character of second line
    assert_eq!(map.offset_to_position(6, source), Position::new(1, 0));
    // First character of third line
    assert_eq!(map.offset_to_position(12, source), Position::new(2, 0));
}

#[test]
fn test_line_map_windows_line_endings() {
    let source = "line1\r\nline2\r\nline3";
    let map = LineMap::build(source);

    assert_eq!(map.line_count(), 3);

    // First character of second line (after \r\n)
    assert_eq!(map.offset_to_position(7, source), Position::new(1, 0));
}

#[test]
fn test_position_to_offset_roundtrip() {
    let source = "const x = 1;\nlet y = 2;\nvar z = 3;";
    let map = LineMap::build(source);

    for offset in 0..u32::try_from(source.len()).unwrap_or_default() {
        let pos = map.offset_to_position(offset, source);
        let back = map.position_to_offset(pos, source).unwrap();
        assert_eq!(offset, back, "roundtrip failed for offset {offset}");
    }
}

#[test]
fn test_utf16_columns() {
    let source = "A 🚀 B";
    let map = LineMap::build(source);

    let pos_rocket = map.offset_to_position(2, source);
    assert_eq!(pos_rocket.character, 2);

    let pos_b = map.offset_to_position(7, source);
    assert_eq!(pos_b.character, 5);

    let offset = map.position_to_offset(Position::new(0, 5), source).unwrap();
    assert_eq!(offset, 7);
}

#[test]
fn test_offset_to_position_inside_supplementary_scalar_uses_utf16_units() {
    let source = "a😀b";
    let map = LineMap::build(source);

    assert_eq!(map.offset_to_position(1, source), Position::new(0, 1));
    assert_eq!(map.offset_to_position(2, source), Position::new(0, 2));
    assert_eq!(map.offset_to_position(3, source), Position::new(0, 2));
    assert_eq!(map.offset_to_position(5, source), Position::new(0, 3));
}

#[test]
fn test_line_start_reports_exact_offsets_and_out_of_range_none() {
    let source = "alpha\nbeta\r\ngamma\rdelta";
    let map = LineMap::build(source);

    assert_eq!(map.line_start(0), Some(0));
    assert_eq!(map.line_start(1), Some(6));
    assert_eq!(map.line_start(2), Some(12));
    assert_eq!(map.line_start(3), Some(18));
    assert_eq!(map.line_start(4), None);
}

#[test]
fn test_offset_to_position_clamps_past_end_of_source() {
    let source = "line one\nline two";
    let map = LineMap::build(source);

    let pos = map.offset_to_position(999, source);
    assert_eq!(pos, Position::new(1, 8));
}

#[test]
fn test_position_to_offset_returns_none_for_missing_line_and_clamps_character() {
    let source = "abc\n🚀";
    let map = LineMap::build(source);

    assert_eq!(map.position_to_offset(Position::new(2, 0), source), None);
    assert_eq!(
        map.position_to_offset(Position::new(1, 10), source),
        Some(8)
    );
}

// =============================================================================
// UTF-8 / UTF-16 width matrix (workstream 8.5)
//
// Lock down `offset_to_position` / `position_to_offset` against each UTF-8
// width class.  The character at the start of each test source is the only
// non-ASCII content on its line, so the byte offsets and UTF-16 columns are
// hand-checkable and small enough to reason about per assertion.
// =============================================================================

#[test]
fn position_two_byte_utf8_latin1_supplement() {
    // `é` is 2 bytes in UTF-8 (0xC3 0xA9), 1 UTF-16 code unit (0x00E9).
    let source = "éxy";
    let map = LineMap::build(source);

    assert_eq!(map.offset_to_position(0, source), Position::new(0, 0));
    assert_eq!(map.offset_to_position(2, source), Position::new(0, 1));
    assert_eq!(map.offset_to_position(3, source), Position::new(0, 2));
    assert_eq!(map.offset_to_position(4, source), Position::new(0, 3));

    assert_eq!(map.position_to_offset(Position::new(0, 0), source), Some(0));
    assert_eq!(map.position_to_offset(Position::new(0, 1), source), Some(2));
    assert_eq!(map.position_to_offset(Position::new(0, 3), source), Some(4));
}

#[test]
fn position_three_byte_utf8_bmp_non_ascii() {
    // `中` is 3 bytes in UTF-8 (0xE4 0xB8 0xAD), 1 UTF-16 code unit (0x4E2D).
    let source = "中a";
    let map = LineMap::build(source);

    assert_eq!(map.offset_to_position(0, source), Position::new(0, 0));
    assert_eq!(map.offset_to_position(3, source), Position::new(0, 1));
    assert_eq!(map.offset_to_position(4, source), Position::new(0, 2));

    assert_eq!(map.position_to_offset(Position::new(0, 0), source), Some(0));
    assert_eq!(map.position_to_offset(Position::new(0, 1), source), Some(3));
    assert_eq!(map.position_to_offset(Position::new(0, 2), source), Some(4));
}

#[test]
fn position_four_byte_utf8_surrogate_pair_boundary() {
    // `🚀` is 4 bytes in UTF-8 (0xF0 0x9F 0x9A 0x80), 2 UTF-16 code units.
    // Position 1 falls inside the surrogate pair — `position_to_offset`
    // must NOT split the codepoint and should clamp to the start byte.
    let source = "🚀";
    let map = LineMap::build(source);

    assert_eq!(map.offset_to_position(0, source), Position::new(0, 0));
    assert_eq!(map.offset_to_position(4, source), Position::new(0, 2));

    assert_eq!(map.position_to_offset(Position::new(0, 0), source), Some(0));
    assert_eq!(
        map.position_to_offset(Position::new(0, 1), source),
        Some(0),
        "splitting inside a surrogate pair must clamp to the start byte"
    );
    assert_eq!(map.position_to_offset(Position::new(0, 2), source), Some(4));
}

#[test]
fn position_mixed_width_classes_round_trip_each_codepoint_boundary() {
    // a:1B/1cu, é:2B/1cu, 中:3B/1cu, 🚀:4B/2cu, b:1B/1cu
    let source = "aé中🚀b";
    let map = LineMap::build(source);

    let codepoint_boundaries = [
        (0, 0),  // start
        (1, 1),  // after `a`
        (3, 2),  // after `é`
        (6, 3),  // after `中`
        (10, 5), // after `🚀` (2 UTF-16 units)
        (11, 6), // after `b`
    ];
    for (offset, character) in codepoint_boundaries {
        let pos = map.offset_to_position(offset as u32, source);
        assert_eq!(
            pos,
            Position::new(0, character),
            "offset_to_position for byte {offset}"
        );
        assert_eq!(
            map.position_to_offset(pos, source),
            Some(offset as u32),
            "position_to_offset round-trip for col {character}"
        );
    }
}

#[test]
fn position_handles_empty_lines_between_content() {
    let source = "first\n\n\nfourth";
    let map = LineMap::build(source);

    assert_eq!(map.line_count(), 4);
    assert_eq!(map.line_start(0), Some(0));
    assert_eq!(map.line_start(1), Some(6));
    assert_eq!(map.line_start(2), Some(7));
    assert_eq!(map.line_start(3), Some(8));

    // Each empty line's only valid position is column 0.
    assert_eq!(map.offset_to_position(6, source), Position::new(1, 0));
    assert_eq!(map.offset_to_position(7, source), Position::new(2, 0));
    assert_eq!(map.position_to_offset(Position::new(1, 0), source), Some(6));
    assert_eq!(map.position_to_offset(Position::new(2, 0), source), Some(7));
}

#[test]
fn position_long_ascii_line_does_not_overflow() {
    // 10_000 ASCII chars in one line — exercises the per-line scan loop
    // without multi-byte branches.
    let source: String = "x".repeat(10_000);
    let map = LineMap::build(&source);

    assert_eq!(map.line_count(), 1);
    assert_eq!(map.offset_to_position(0, &source), Position::new(0, 0));
    assert_eq!(
        map.offset_to_position(10_000, &source),
        Position::new(0, 10_000)
    );
    assert_eq!(
        map.position_to_offset(Position::new(0, 5_000), &source),
        Some(5_000)
    );
}

#[test]
fn position_4byte_utf8_after_newline_keeps_line_alignment() {
    // Multi-byte chars on a non-first line — make sure the line-start
    // anchor is correct and the per-line scan begins fresh.
    let source = "ascii\n🚀x";
    let map = LineMap::build(source);

    assert_eq!(map.line_count(), 2);
    assert_eq!(map.line_start(1), Some(6));

    // Byte 6 is the first byte of `🚀`.
    assert_eq!(map.offset_to_position(6, source), Position::new(1, 0));
    assert_eq!(map.offset_to_position(10, source), Position::new(1, 2));
    assert_eq!(map.offset_to_position(11, source), Position::new(1, 3));

    assert_eq!(
        map.position_to_offset(Position::new(1, 2), source),
        Some(10)
    );
    assert_eq!(
        map.position_to_offset(Position::new(1, 3), source),
        Some(11)
    );
}
