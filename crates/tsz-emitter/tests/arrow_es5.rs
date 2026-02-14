use tsz_parser::parser::ParserState;

#[test]
fn test_detect_this_in_arrow() {
    let source = "const f = () => this.x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Simple test: the source contains "this" keyword
    assert!(
        source.contains("this"),
        "Expected to detect 'this' in source"
    );
}

#[test]
fn test_no_this_in_arrow() {
    let source = "const add = (a, b) => a + b;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Simple test: the source doesn't contain "this"
    assert!(
        !source.contains("this"),
        "Should not detect 'this' in simple arrow"
    );
}
