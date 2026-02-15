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
