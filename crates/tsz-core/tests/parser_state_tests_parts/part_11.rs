#[test]
fn test_parser_this_type_predicate() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function isString(this: any): this is string { return true; }".to_string(),
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
fn test_parser_asserts_this_type_predicate() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function assertString(this: any): asserts this is string { }".to_string(),
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
fn test_parser_asserts_this_type_predicate_without_is() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function assertThis(this: any): asserts this { }".to_string(),
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
fn test_parser_constructor_type() {
    // Constructor type: new () => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Ctor = new () => object;".to_string(),
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
fn test_parser_constructor_type_with_params() {
    // Constructor type with parameters: new (x: T) => U
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Factory<T> = new (value: T) => Wrapper<T>;".to_string(),
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
fn test_parser_generic_constructor_type() {
    // Generic constructor type: new <T>() => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type GenericCtor = new <T>() => T;".to_string(),
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
// Type Operator Tests (keyof, readonly)
// =========================================================================

#[test]
fn test_parser_keyof_type() {
    // Basic keyof type: keyof T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Keys = keyof Person;".to_string(),
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
fn test_parser_keyof_typeof() {
    // keyof typeof: keyof typeof obj
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Keys = keyof typeof obj;".to_string(),
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
fn test_parser_keyof_in_union() {
    // keyof in union type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type PropOrKey = string | keyof T;".to_string(),
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
fn test_parser_readonly_array() {
    // readonly array type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let items: readonly string[];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

