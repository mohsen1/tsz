#[test]
fn test_parser_readonly_index_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface ReadonlyMap { readonly [key: string]: string; }".to_string(),
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
fn test_parser_readonly_property_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Config { readonly name: string; readonly value: number; }".to_string(),
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
fn test_parser_arrow_function_simple() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const add = (a, b) => a + b;".to_string(),
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
fn test_parser_arrow_function_single_param() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const double = x => x * 2;".to_string(),
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
fn test_parser_arrow_function_block_body() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const greet = (name) => { return name; };".to_string(),
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
fn test_parser_arrow_function_no_params() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const getTime = () => Date.now();".to_string(),
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
fn test_parser_arrow_function_in_object_literal() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const obj = { handler: () => { }, value: 1 };".to_string(),
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
fn test_parser_type_assertion_angle_bracket() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const value = <number>someValue;".to_string(),
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
fn test_parser_literal_type_assertion_angle_bracket() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const value = <\"ok\">someValue;".to_string(),
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
fn test_parser_async_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "async function fetchData() { return await fetch(); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

