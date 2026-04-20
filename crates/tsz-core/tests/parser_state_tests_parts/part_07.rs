#[test]
fn test_parser_async_arrow_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const fetchData = async () => await fetch();".to_string(),
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
fn test_parser_async_arrow_single_param() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const processItem = async item => await process(item);".to_string(),
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
fn test_parser_generator_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function* range(n) { for (let i = 0; i < n; i++) yield i; }".to_string(),
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
fn test_parser_yield_expression() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function* gen() { yield 1; yield 2; }".to_string(),
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
fn test_parser_yield_star() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function* delegate() { yield* otherGen(); }".to_string(),
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
fn test_parser_await_expression() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "async function test() { const x = await promise; }".to_string(),
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
fn test_parser_union_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let x: string | number | boolean;".to_string(),
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
fn test_parser_intersection_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "let x: A & B & C;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_union_intersection_mixed() {
    // Intersection binds tighter than union: A & B | C means (A & B) | C
    let mut parser = ParserState::new("test.ts".to_string(), "let x: A & B | C & D;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_array_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "let arr: string[];".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

