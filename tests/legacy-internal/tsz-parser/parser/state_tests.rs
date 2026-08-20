use super::ParserState;

#[test]
fn u32_from_usize_clamps_overflow_without_panicking() {
    let parser = ParserState::new("a.ts".to_string(), String::new());

    assert_eq!(parser.u32_from_usize(usize::MAX), u32::MAX);
    assert!(parser.reported_offset_overflow.get());
}

#[test]
fn u16_from_node_flags_truncates_overflow_without_panicking() {
    let parser = ParserState::new("a.ts".to_string(), String::new());

    assert_eq!(parser.u16_from_node_flags(0x1_0001), 1);
    assert!(parser.reported_node_flag_overflow.get());
}

#[test]
fn reset_clears_conversion_overflow_markers() {
    let mut parser = ParserState::new("a.ts".to_string(), String::new());
    let _ = parser.u32_from_usize(usize::MAX);
    let _ = parser.u16_from_node_flags(0x1_0001);

    assert!(parser.reported_offset_overflow.get());
    assert!(parser.reported_node_flag_overflow.get());

    parser.reset("b.ts".to_string(), String::new());

    assert!(!parser.reported_offset_overflow.get());
    assert!(!parser.reported_node_flag_overflow.get());
}
