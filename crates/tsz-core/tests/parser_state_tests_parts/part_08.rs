#[test]
fn test_parser_nested_array_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "let matrix: number[][];".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_union_array_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let items: (string | number)[];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_tuple_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let point: [number, number];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_tuple_type_mixed() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let result: [string, number, boolean];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_tuple_array() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let points: [number, number][];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let list: Array<string>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_type_multiple() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let map: Map<string, number>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_nested() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let nested: Map<string, Array<number>>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_promise_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "async function fetch(): Promise<string> { return ''; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_function_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let callback: (x: number) => string;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

