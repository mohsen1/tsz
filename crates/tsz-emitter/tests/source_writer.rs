use super::*;
use crate::emitter::NewLineKind;

#[test]
fn test_basic_write() {
    let mut writer = SourceWriter::new();
    writer.write("hello");
    writer.write(" ");
    writer.write("world");
    assert_eq!(writer.get_output(), "hello world");
}

#[test]
fn test_newline_tracking() {
    let mut writer = SourceWriter::new();
    writer.write("line 1");
    writer.write_line();
    writer.write("line 2");

    assert_eq!(writer.current_line(), 1);
    assert_eq!(writer.get_output(), "line 1\nline 2");
}

#[test]
fn test_indentation() {
    let mut writer = SourceWriter::new();
    writer.write("start");
    writer.write_line();
    writer.increase_indent();
    writer.write("indented");
    writer.write_line();
    writer.decrease_indent();
    writer.write("back");

    assert_eq!(writer.get_output(), "start\n    indented\nback");
}

#[test]
fn test_compute_line_col() {
    let text = "line1\nline2\nline3";

    assert_eq!(compute_line_col(text, 0), (0, 0)); // 'l' of line1
    assert_eq!(compute_line_col(text, 5), (0, 5)); // '\n' after line1
    assert_eq!(compute_line_col(text, 6), (1, 0)); // 'l' of line2
    assert_eq!(compute_line_col(text, 12), (2, 0)); // 'l' of line3
}

#[test]
fn test_undo_last_write_line_restores_previous_state() {
    let mut writer = SourceWriter::new();
    writer.write("hello");
    writer.write_line();

    assert_eq!(writer.current_line(), 1);
    assert!(writer.undo_last_write_line());
    assert_eq!(writer.get_output(), "hello");
    assert_eq!(writer.current_line(), 0);
    assert_eq!(writer.current_column(), 5);
    assert!(!writer.is_at_line_start());
    assert!(!writer.undo_last_write_line());
}

#[test]
fn test_insert_line_at_shifts_output_and_source_maps() {
    let mut writer = SourceWriter::with_source_map("out.js".to_string());
    writer.add_source("input.ts".to_string(), None);

    writer.write_node(
        "alpha",
        SourcePosition {
            pos: 0,
            line: 0,
            column: 0,
        },
    );
    writer.write_line();
    writer.write_node(
        "beta",
        SourcePosition {
            pos: 6,
            line: 1,
            column: 0,
        },
    );

    writer.insert_line_at(0, 0, "var _a;");

    assert_eq!(writer.get_output(), "var _a;\nalpha\nbeta");
    assert_eq!(writer.current_line(), 2);

    let map = writer
        .take_source_map()
        .expect("source map should be enabled")
        .generate();
    assert!(map.mappings.starts_with(';'));
    assert!(map.mappings.matches(';').count() >= 2);
}

// =============================================================================
// SourceWriter - constructors and defaults
// =============================================================================

#[test]
fn new_writer_starts_at_origin() {
    let writer = SourceWriter::new();
    assert_eq!(writer.current_line(), 0);
    assert_eq!(writer.current_column(), 0);
    assert_eq!(writer.indent_level(), 0);
    assert!(writer.is_at_line_start());
    assert!(writer.is_empty());
    assert_eq!(writer.len(), 0);
    assert!(!writer.has_source_map());
    assert_eq!(writer.current_source_index(), 0);
}

#[test]
fn default_matches_new() {
    let a = SourceWriter::default();
    let b = SourceWriter::new();
    assert_eq!(a.is_empty(), b.is_empty());
    assert_eq!(a.indent_level(), b.indent_level());
    assert_eq!(a.is_at_line_start(), b.is_at_line_start());
    assert_eq!(a.current_line(), b.current_line());
    assert_eq!(a.current_column(), b.current_column());
}

#[test]
fn with_capacity_reserves_buffer() {
    let writer = SourceWriter::with_capacity(8192);
    assert!(writer.capacity() >= 8192);
    assert_eq!(writer.len(), 0);
}

#[test]
fn with_source_map_enables_source_map_mode() {
    let writer = SourceWriter::with_source_map("out.js".to_string());
    assert!(writer.has_source_map());
}

#[test]
fn enable_source_map_is_idempotent() {
    let mut writer = SourceWriter::new();
    assert!(!writer.has_source_map());
    writer.enable_source_map("out.js".to_string());
    assert!(writer.has_source_map());
    // Calling again does not toggle off or replace.
    writer.enable_source_map("ignored.js".to_string());
    assert!(writer.has_source_map());
}

// =============================================================================
// SourceWriter - text writing and column tracking
// =============================================================================

#[test]
fn write_updates_column_for_ascii() {
    let mut writer = SourceWriter::new();
    writer.write("abc");
    assert_eq!(writer.current_column(), 3);
    assert_eq!(writer.current_line(), 0);
    assert!(!writer.is_at_line_start());
}

#[test]
fn write_char_emits_single_character() {
    let mut writer = SourceWriter::new();
    writer.write_char('x');
    writer.write_char('y');
    writer.write_char('z');
    assert_eq!(writer.get_output(), "xyz");
    assert_eq!(writer.current_column(), 3);
}

#[test]
fn write_char_handles_newline() {
    let mut writer = SourceWriter::new();
    writer.write("ab");
    writer.write_char('\n');
    writer.write("c");
    assert_eq!(writer.get_output(), "ab\nc");
    assert_eq!(writer.current_line(), 1);
    assert_eq!(writer.current_column(), 1);
}

#[test]
fn write_space_writes_one_space() {
    let mut writer = SourceWriter::new();
    writer.write("a");
    writer.write_space();
    writer.write("b");
    assert_eq!(writer.get_output(), "a b");
    assert_eq!(writer.current_column(), 3);
}

#[test]
fn write_with_internal_newlines_resets_column() {
    let mut writer = SourceWriter::new();
    writer.write("foo\nbar");
    assert_eq!(writer.current_line(), 1);
    assert_eq!(writer.current_column(), 3);
}

#[test]
fn write_multibyte_utf16_column_tracking() {
    // BMP 3-byte UTF-8 characters count as 1 UTF-16 unit each.
    let mut writer = SourceWriter::new();
    writer.write("\u{4E2D}\u{6587}"); // 中文
    assert_eq!(writer.current_column(), 2);
}

#[test]
fn write_supplementary_codepoint_counts_as_two_utf16_units() {
    // 🦀 is U+1F980 (non-BMP, surrogate pair in UTF-16 -> 2 code units).
    let mut writer = SourceWriter::new();
    writer.write("\u{1F980}");
    assert_eq!(writer.current_column(), 2);
}

#[test]
fn write_char_supplementary_counts_as_two_utf16_units() {
    let mut writer = SourceWriter::new();
    writer.write_char('\u{1F600}'); // 😀
    assert_eq!(writer.current_column(), 2);
}

#[test]
fn write_usize_zero_emits_single_digit() {
    let mut writer = SourceWriter::new();
    writer.write_usize(0);
    assert_eq!(writer.get_output(), "0");
    assert_eq!(writer.current_column(), 1);
}

#[test]
fn write_usize_emits_decimal_digits() {
    let mut writer = SourceWriter::new();
    writer.write_usize(12345);
    assert_eq!(writer.get_output(), "12345");
    assert_eq!(writer.current_column(), 5);
}

#[test]
fn write_raw_text_skips_indentation() {
    let mut writer = SourceWriter::new();
    writer.increase_indent();
    writer.increase_indent();
    writer.write_raw_text("nope");
    assert_eq!(writer.get_output(), "nope");
}

// =============================================================================
// SourceWriter - line and indentation
// =============================================================================

#[test]
fn write_line_resets_column_and_marks_line_start() {
    let mut writer = SourceWriter::new();
    writer.write("hello");
    writer.write_line();
    assert_eq!(writer.current_column(), 0);
    assert!(writer.is_at_line_start());
    assert_eq!(writer.current_line(), 1);
}

#[test]
fn increase_decrease_indent_level() {
    let mut writer = SourceWriter::new();
    writer.increase_indent();
    writer.increase_indent();
    writer.increase_indent();
    assert_eq!(writer.indent_level(), 3);
    writer.decrease_indent();
    assert_eq!(writer.indent_level(), 2);
}

#[test]
fn decrease_indent_below_zero_saturates() {
    let mut writer = SourceWriter::new();
    writer.decrease_indent();
    writer.decrease_indent();
    assert_eq!(writer.indent_level(), 0);
}

#[test]
fn set_indent_level_overrides_directly() {
    let mut writer = SourceWriter::new();
    writer.set_indent_level(7);
    assert_eq!(writer.indent_level(), 7);
}

#[test]
fn indent_width_uses_indent_str_length() {
    let mut writer = SourceWriter::new();
    writer.set_indent_str("\t"); // 1 char
    writer.set_indent_level(3);
    assert_eq!(writer.indent_width(), 3);

    writer.set_indent_str("  "); // 2 chars
    writer.set_indent_level(4);
    assert_eq!(writer.indent_width(), 8);
}

#[test]
fn set_indent_str_changes_emitted_indent() {
    let mut writer = SourceWriter::new();
    writer.set_indent_str("  ");
    writer.write_line();
    writer.increase_indent();
    writer.write("x");
    assert_eq!(writer.get_output(), "\n  x");
}

#[test]
fn lazy_indent_only_emits_on_first_write_after_newline() {
    let mut writer = SourceWriter::new();
    writer.increase_indent();
    // No write yet, so no indent emitted.
    assert_eq!(writer.get_output(), "");
    writer.write("hi");
    assert_eq!(writer.get_output(), "    hi");
    // After the first write, subsequent writes on the same line do NOT re-indent.
    writer.write("there");
    assert_eq!(writer.get_output(), "    hithere");
}

// =============================================================================
// SourceWriter - new line kinds
// =============================================================================

#[test]
fn set_new_line_kind_lf_uses_unix_newline() {
    let mut writer = SourceWriter::new();
    writer.set_new_line_kind(NewLineKind::LineFeed);
    writer.write("a");
    writer.write_line();
    writer.write("b");
    assert_eq!(writer.get_output(), "a\nb");
}

#[test]
fn set_new_line_kind_crlf_uses_windows_newline() {
    let mut writer = SourceWriter::new();
    writer.set_new_line_kind(NewLineKind::CarriageReturnLineFeed);
    writer.write("a");
    writer.write_line();
    writer.write("b");
    assert_eq!(writer.get_output(), "a\r\nb");
    assert_eq!(writer.current_line(), 1);
}

#[test]
fn undo_last_write_line_handles_crlf() {
    let mut writer = SourceWriter::new();
    writer.set_new_line_kind(NewLineKind::CarriageReturnLineFeed);
    writer.write("hello");
    writer.write_line();
    assert_eq!(writer.current_line(), 1);
    assert!(writer.undo_last_write_line());
    assert_eq!(writer.get_output(), "hello");
    assert_eq!(writer.current_line(), 0);
    assert_eq!(writer.current_column(), 5);
    assert!(!writer.is_at_line_start());
}

#[test]
fn undo_last_write_line_returns_false_at_start_of_buffer() {
    let mut writer = SourceWriter::new();
    assert!(!writer.undo_last_write_line());
    writer.write("hi");
    // No newline has been written yet, undo should be a no-op.
    assert!(!writer.undo_last_write_line());
    assert_eq!(writer.get_output(), "hi");
}

// =============================================================================
// SourceWriter - last_non_whitespace_byte
// =============================================================================

#[test]
fn last_non_whitespace_byte_skips_trailing_whitespace() {
    let mut writer = SourceWriter::new();
    writer.write("ab   ");
    assert_eq!(writer.last_non_whitespace_byte(), Some(b'b'));
    writer.write("\n\t  ");
    assert_eq!(writer.last_non_whitespace_byte(), Some(b'b'));
}

#[test]
fn last_non_whitespace_byte_none_when_only_whitespace() {
    let mut writer = SourceWriter::new();
    writer.write("   \n\t");
    assert!(writer.last_non_whitespace_byte().is_none());
}

#[test]
fn last_non_whitespace_byte_none_when_empty() {
    let writer = SourceWriter::new();
    assert!(writer.last_non_whitespace_byte().is_none());
}

// =============================================================================
// SourceWriter - take_output / capacity
// =============================================================================

#[test]
fn take_output_yields_owned_string() {
    let mut writer = SourceWriter::new();
    writer.write("abc");
    let s = writer.take_output();
    assert_eq!(s, "abc");
}

#[test]
fn ensure_output_capacity_grows_when_needed() {
    let mut writer = SourceWriter::with_capacity(8);
    writer.ensure_output_capacity(1024);
    assert!(writer.capacity() >= 1024);
}

#[test]
fn ensure_output_capacity_no_shrink() {
    let mut writer = SourceWriter::with_capacity(4096);
    let before = writer.capacity();
    writer.ensure_output_capacity(16);
    // Should not shrink the buffer.
    assert!(writer.capacity() >= before);
}

// =============================================================================
// SourceWriter - truncate
// =============================================================================

#[test]
fn truncate_to_zero_resets_state() {
    let mut writer = SourceWriter::new();
    writer.write("abc\nxyz");
    writer.truncate(0);
    assert!(writer.is_empty());
    assert_eq!(writer.current_line(), 0);
    assert_eq!(writer.current_column(), 0);
    assert!(writer.is_at_line_start());
}

#[test]
fn truncate_to_after_newline_marks_line_start() {
    let mut writer = SourceWriter::new();
    writer.write("hi");
    writer.write_line();
    writer.write("more");
    writer.truncate(3); // "hi\n"
    assert_eq!(writer.get_output(), "hi\n");
    assert_eq!(writer.current_line(), 1);
    assert_eq!(writer.current_column(), 0);
    assert!(writer.is_at_line_start());
}

#[test]
fn truncate_mid_line_recomputes_column() {
    let mut writer = SourceWriter::new();
    writer.write("hello\nworld!");
    writer.truncate(8); // "hello\nwo"
    assert_eq!(writer.get_output(), "hello\nwo");
    assert_eq!(writer.current_line(), 1);
    assert_eq!(writer.current_column(), 2);
    assert!(!writer.is_at_line_start());
}

// =============================================================================
// SourceWriter - insert_at
// =============================================================================

#[test]
fn insert_at_injects_inline_text_without_shifting_lines() {
    let mut writer = SourceWriter::new();
    writer.write("function f() { return 0; }");
    let line_before = writer.current_line();
    writer.insert_at(15, "var _a; ");
    assert_eq!(writer.get_output(), "function f() { var _a; return 0; }");
    // insert_at must not change the line counter.
    assert_eq!(writer.current_line(), line_before);
}

// =============================================================================
// SourceWriter - source map ops
// =============================================================================

#[test]
fn add_source_returns_zero_when_source_map_disabled() {
    let mut writer = SourceWriter::new();
    let idx = writer.add_source("input.ts".to_string(), None);
    assert_eq!(idx, 0);
}

#[test]
fn add_source_returns_index_when_source_map_enabled() {
    let mut writer = SourceWriter::with_source_map("out.js".to_string());
    let idx = writer.add_source("input.ts".to_string(), None);
    assert_eq!(idx, 0);
    assert_eq!(writer.current_source_index(), 0);
    let idx2 = writer.add_source("other.ts".to_string(), Some("x".to_string()));
    assert_eq!(idx2, 1);
    assert_eq!(writer.current_source_index(), 1);
}

#[test]
fn write_node_without_source_map_only_writes_text() {
    let mut writer = SourceWriter::new();
    writer.write_node(
        "alpha",
        SourcePosition {
            pos: 0,
            line: 0,
            column: 0,
        },
    );
    assert_eq!(writer.get_output(), "alpha");
}

#[test]
fn write_node_with_end_emits_two_mappings() {
    let mut writer = SourceWriter::with_source_map("out.js".to_string());
    writer.add_source("input.ts".to_string(), None);
    writer.write_node_with_end(
        ";",
        SourcePosition {
            pos: 0,
            line: 0,
            column: 5,
        },
    );
    let map = writer
        .take_source_map()
        .expect("source map enabled")
        .generate();
    assert!(!map.mappings.is_empty());
}

#[test]
fn write_node_usize_writes_digits_and_records_mapping() {
    let mut writer = SourceWriter::with_source_map("out.js".to_string());
    writer.add_source("input.ts".to_string(), None);
    writer.write_node_usize(
        42,
        SourcePosition {
            pos: 10,
            line: 0,
            column: 10,
        },
    );
    assert_eq!(writer.get_output(), "42");
    assert_eq!(writer.current_column(), 2);
    let map = writer
        .take_source_map()
        .expect("source map enabled")
        .generate();
    assert!(!map.mappings.is_empty());
}

#[test]
fn write_node_with_name_records_named_mapping() {
    let mut writer = SourceWriter::with_source_map("out.js".to_string());
    writer.add_source("input.ts".to_string(), None);
    writer.write_node_with_name(
        "foo",
        SourcePosition {
            pos: 0,
            line: 0,
            column: 0,
        },
        "foo",
    );
    let map = writer
        .take_source_map()
        .expect("source map enabled")
        .generate();
    assert!(map.names.iter().any(|n| n == "foo"));
}

#[test]
fn generate_source_map_json_returns_some_when_enabled() {
    let mut writer = SourceWriter::with_source_map("out.js".to_string());
    writer.add_source("input.ts".to_string(), None);
    writer.write_node(
        "a",
        SourcePosition {
            pos: 0,
            line: 0,
            column: 0,
        },
    );
    let json = writer.generate_source_map_json().expect("json available");
    assert!(json.contains("\"version\":3"));
    assert!(json.contains("out.js"));
}

#[test]
fn generate_source_map_json_returns_none_when_disabled() {
    let mut writer = SourceWriter::new();
    assert!(writer.generate_source_map_json().is_none());
}

#[test]
fn take_source_map_returns_none_when_disabled() {
    let writer = SourceWriter::new();
    assert!(writer.take_source_map().is_none());
}

#[test]
fn add_offset_mappings_no_op_when_source_map_disabled() {
    let mut writer = SourceWriter::new();
    let mappings = vec![tsz_common::source_map::Mapping {
        generated_line: 0,
        generated_column: 0,
        source_index: 0,
        original_line: 0,
        original_column: 0,
        name_index: None,
    }];
    // Should not panic. There's no observable side effect to assert beyond no panic.
    writer.add_offset_mappings(0, 0, &mappings);
    assert!(writer.generate_source_map_json().is_none());
}

#[test]
fn add_mappings_with_line_column_offset_records_shifted_mappings() {
    let mut writer = SourceWriter::with_source_map("out.js".to_string());
    writer.add_source("input.ts".to_string(), None);
    let base_mappings = vec![
        tsz_common::source_map::Mapping {
            generated_line: 0,
            generated_column: 0,
            source_index: 0,
            original_line: 0,
            original_column: 0,
            name_index: None,
        },
        tsz_common::source_map::Mapping {
            generated_line: 1,
            generated_column: 2,
            source_index: 0,
            original_line: 1,
            original_column: 4,
            name_index: None,
        },
    ];
    writer.add_mappings_with_line_column_offset(5, 3, &base_mappings);
    let map = writer
        .take_source_map()
        .expect("source map enabled")
        .generate();
    // Mappings string should be non-empty.
    assert!(!map.mappings.is_empty());
}

// =============================================================================
// LineMap
// =============================================================================

#[test]
fn line_map_single_line_offset() {
    let lm = LineMap::new("hello world");
    assert_eq!(lm.line_col(0), (0, 0));
    assert_eq!(lm.line_col(5), (0, 5));
    assert_eq!(lm.line_col(10), (0, 10));
}

#[test]
fn line_map_multi_line_offsets() {
    let lm = LineMap::new("ab\ncd\nef");
    assert_eq!(lm.line_col(0), (0, 0));
    assert_eq!(lm.line_col(2), (0, 2)); // before \n
    assert_eq!(lm.line_col(3), (1, 0));
    assert_eq!(lm.line_col(5), (1, 2));
    assert_eq!(lm.line_col(6), (2, 0));
    assert_eq!(lm.line_col(7), (2, 1));
}

#[test]
fn line_map_at_newline_byte() {
    let lm = LineMap::new("a\nb");
    // Position 1 is '\n' which sits on line 0 column 1.
    assert_eq!(lm.line_col(1), (0, 1));
    // Position 2 is the next 'b' on line 1.
    assert_eq!(lm.line_col(2), (1, 0));
}

#[test]
fn line_map_position_past_end_returns_eof_position() {
    let lm = LineMap::new("xyz");
    let (line, col) = lm.line_col(100);
    // Single line with three chars; col counts UTF-16 code units of last line.
    assert_eq!(line, 0);
    assert_eq!(col, 3);
}

#[test]
fn line_map_position_past_end_multiline_returns_last_line() {
    let lm = LineMap::new("aa\nbbb");
    let (line, col) = lm.line_col(100);
    assert_eq!(line, 1);
    assert_eq!(col, 3);
}

#[test]
fn line_map_empty_text_returns_origin() {
    let lm = LineMap::new("");
    assert_eq!(lm.line_col(0), (0, 0));
    assert_eq!(lm.line_col(100), (0, 0));
}

#[test]
fn line_map_utf16_columns_for_supplementary_codepoint() {
    let lm = LineMap::new("\u{1F600}x"); // 😀x — emoji is 2 UTF-16 units, 4 UTF-8 bytes.
    // Position 4 is right after the emoji (start of 'x').
    let (line, col) = lm.line_col(4);
    assert_eq!(line, 0);
    assert_eq!(col, 2);
    // After the 'x' at position 5.
    let (line2, col2) = lm.line_col(5);
    assert_eq!(line2, 0);
    assert_eq!(col2, 3);
}

#[test]
fn line_map_source_position_returns_struct_form() {
    let lm = LineMap::new("ab\ncd");
    let pos = lm.source_position(4);
    assert_eq!(pos.pos, 4);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
}

// =============================================================================
// compute_line_col / source_position_from_offset
// =============================================================================

#[test]
fn compute_line_col_position_past_end() {
    let text = "ab\ncd";
    let (line, col) = compute_line_col(text, 100);
    assert_eq!(line, 1);
    assert_eq!(col, 2);
}

#[test]
fn compute_line_col_empty_text() {
    let (line, col) = compute_line_col("", 0);
    assert_eq!(line, 0);
    assert_eq!(col, 0);
}

#[test]
fn compute_line_col_supplementary_codepoint_counts_two() {
    let text = "\u{1F600}rest"; // emoji + "rest"
    // Position 4 is right after the emoji (start of 'r').
    let (line, col) = compute_line_col(text, 4);
    assert_eq!(line, 0);
    assert_eq!(col, 2);
}

#[test]
fn source_position_from_offset_round_trip_simple() {
    let pos = source_position_from_offset("abc\ndef", 5);
    assert_eq!(pos.pos, 5);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
}

#[test]
fn source_position_default_is_origin() {
    let pos: SourcePosition = SourcePosition::default();
    assert_eq!(pos.pos, 0);
    assert_eq!(pos.line, 0);
    assert_eq!(pos.column, 0);
}

#[test]
fn source_position_copy_clone_independence() {
    let a = SourcePosition {
        pos: 5,
        line: 2,
        column: 3,
    };
    let b = a;
    let c = a;
    assert_eq!(b.pos, 5);
    assert_eq!(c.line, 2);
    assert_eq!(a.column, 3);
}
