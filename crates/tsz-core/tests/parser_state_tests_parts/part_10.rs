#[test]
fn test_parser_generic_arrow_simple() {
    // Basic generic arrow function: <T>(x: T) => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const identity = <T>(x: T) => x;".to_string(),
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
fn test_parser_generic_arrow_tsx_trailing_comma() {
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const id = <T,>(x: T): T => x;".to_string(),
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
fn test_parser_generic_arrow_multiple_params() {
    // Multiple type parameters: <T, U>(x: T, y: U) => [T, U]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const pair = <T, U>(x: T, y: U) => [x, y];".to_string(),
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
fn test_parser_generic_arrow_with_constraint() {
    // Type parameter with constraint: <T extends object>(x: T) => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const clone = <T extends object>(x: T) => x;".to_string(),
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
fn test_parser_generic_arrow_with_default() {
    // Type parameter with default: <T = string>(x: T) => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const wrap = <T = string>(x: T) => x;".to_string(),
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
fn test_parser_generic_arrow_with_constraint_and_default() {
    // Type parameter with both constraint and default
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const process = <T extends object = object>(x: T) => x;".to_string(),
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
fn test_parser_async_generic_arrow() {
    // Async generic arrow function
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const fetchData = async <T>(url: string) => { return url; };".to_string(),
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
fn test_parser_generic_arrow_expression_body() {
    // Generic arrow with expression body
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const first = <T>(arr: T[]) => arr[0];".to_string(),
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
fn test_parser_arrow_function_with_return_type() {
    // Arrow function with return type annotation
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const add = (a: number, b: number): number => a + b;".to_string(),
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
fn test_parser_arrow_type_predicate() {
    // Arrow function with type predicate return type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const isString = (x: unknown): x is string => typeof x === \"string\";".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

