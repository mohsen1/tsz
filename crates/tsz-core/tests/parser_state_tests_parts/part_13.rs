#[test]
fn test_parser_infer_type() {
    // Infer in array element position
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Flatten<T> = T extends Array<infer U> ? U : T;".to_string(),
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
// Mapped Type Tests
// =========================================================================

#[test]
fn test_parser_mapped_type_simple() {
    // Basic mapped type: { [K in keyof T]: U }
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Partial<T> = { [K in keyof T]?: T[K] };".to_string(),
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
fn test_parser_mapped_type_readonly() {
    // Mapped type with readonly modifier
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Readonly<T> = { readonly [K in keyof T]: T[K] };".to_string(),
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
fn test_parser_mapped_type_required() {
    // Mapped type removing optional: -?
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Required<T> = { [K in keyof T]-?: T[K] };".to_string(),
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
fn test_parser_mapped_type_as_clause() {
    // Mapped type with key remapping (as clause)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Pick<T, K> = { [P in K as P]: T[P] };".to_string(),
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
fn test_parser_type_literal() {
    // Object type literal
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Point = { x: number; y: number };".to_string(),
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
fn test_parser_type_literal_method() {
    // Object type literal with method signature
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Calculator = { add(a: number, b: number): number; subtract(a: number, b: number): number };".to_string(),
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
// Template Literal Type Tests
// =========================================================================

#[test]
fn test_parser_template_literal_type_simple() {
    // Simple template literal type with no substitutions
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Greeting = `hello`;".to_string(),
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
fn test_parser_template_literal_type_with_substitution() {
    // Template literal type with type substitution
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Greeting<T extends string> = `hello ${T}`;".to_string(),
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
fn test_parser_template_literal_type_multiple_substitutions() {
    // Template literal type with multiple substitutions
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type FullName<F extends string, L extends string> = `${F} ${L}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

