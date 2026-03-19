use super::*;

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
