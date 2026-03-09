use crate::source_writer::{SourcePosition, SourceWriter};

#[test]
fn write_usize_emits_digits() {
    let mut writer = SourceWriter::new();
    writer.write_usize(0);
    writer.write(",");
    writer.write_usize(42);
    assert_eq!(writer.get_output(), "0,42");
}

#[test]
fn write_node_usize_emits_digits_and_mapping() {
    let mut writer = SourceWriter::with_source_map("out.js".to_string());
    writer.add_source("test.ts".to_string(), None);
    writer.write_node_usize(
        123,
        SourcePosition {
            pos: 0,
            line: 0,
            column: 0,
        },
    );
    assert_eq!(writer.get_output(), "123");
    let map_json = writer.generate_source_map_json().expect("source map");
    assert!(
        map_json.contains("\"mappings\""),
        "expected mappings in source map json: {map_json}"
    );
}

#[test]
fn ensure_output_capacity_grows() {
    let mut writer = SourceWriter::new();
    let base = writer.capacity();
    let target = base + 1024;
    writer.ensure_output_capacity(target);
    assert!(writer.capacity() >= target);
}
