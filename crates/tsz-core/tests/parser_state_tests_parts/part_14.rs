#[test]
fn test_parser_template_literal_type_with_union() {
    // Template literal type with union in substitution
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type EventName = `on${\"click\" | \"focus\" | \"blur\"}`;".to_string(),
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
fn test_parser_template_literal_type_uppercase() {
    // Template literal type with intrinsic type (Uppercase)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Getter<K extends string> = `get${Uppercase<K>}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// JSX Tests
// =========================================================================

#[test]
fn test_parser_jsx_self_closing() {
    // Self-closing JSX element
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <Component />;".to_string(),
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
fn test_parser_jsx_with_children() {
    // JSX element with children
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <div><span /></div>;".to_string(),
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
fn test_parser_jsx_with_attributes() {
    // JSX with attributes
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <div className=\"foo\" id={bar} disabled />;".to_string(),
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
fn test_parser_jsx_with_expression() {
    // JSX with expression children
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <div>{items.map(i => <span>{i}</span>)}</div>;".to_string(),
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
fn test_parser_jsx_fragment() {
    // JSX fragment
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <><span /><span /></>;".to_string(),
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
fn test_parser_jsx_spread_attribute() {
    // JSX with spread attribute
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <Component {...props} />;".to_string(),
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
fn test_parser_jsx_namespaced() {
    // JSX with namespaced tag name
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <svg:rect width={100} />;".to_string(),
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
fn test_parser_jsx_member_expression() {
    // JSX with member expression tag
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const x = <Foo.Bar.Baz />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Import/Export Tests
// =========================================================================

