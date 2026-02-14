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
