use crate::source_writer::SourceWriter;

#[test]
fn write_usize_emits_digits() {
    let mut writer = SourceWriter::new();
    writer.write_usize(0);
    writer.write(",");
    writer.write_usize(42);
    assert_eq!(writer.get_output(), "0,42");
}

#[test]
fn ensure_output_capacity_grows() {
    let mut writer = SourceWriter::new();
    let base = writer.capacity();
    let target = base + 1024;
    writer.ensure_output_capacity(target);
    assert!(writer.capacity() >= target);
}
