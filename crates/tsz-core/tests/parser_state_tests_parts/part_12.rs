#[test]
fn test_parser_readonly_tuple() {
    // readonly tuple type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let point: readonly [number, number];".to_string(),
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
// Indexed Access Type Tests
// =========================================================================

#[test]
fn test_parser_indexed_access_type() {
    // Basic indexed access: T[K]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Value = Person[\"name\"];".to_string(),
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
fn test_parser_indexed_access_keyof() {
    // Indexed access with keyof: T[keyof T]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Values = Person[keyof Person];".to_string(),
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
fn test_parser_indexed_access_chain() {
    // Chained indexed access: T[K1][K2]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Deep = Obj[\"level1\"][\"level2\"];".to_string(),
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
fn test_parser_indexed_access_with_array() {
    // Mix of indexed access and array: T[K][]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Names = Person[\"name\"][];".to_string(),
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
fn test_parser_indexed_access_number() {
    // Indexed access with number: T[number]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type Item = Items[number];".to_string(),
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
// Conditional Type Tests
// =========================================================================

#[test]
fn test_parser_conditional_type_simple() {
    // Basic conditional type: T extends U ? X : Y
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type IsString<T> = T extends string ? true : false;".to_string(),
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
fn test_parser_conditional_type_nested() {
    // Nested conditional types
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type TypeName<T> = T extends string ? \"string\" : T extends number ? \"number\" : \"other\";".to_string(),
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
fn test_parser_conditional_type_with_infer() {
    // Conditional type with infer
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never;".to_string(),
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
fn test_parser_conditional_type_distributive() {
    // Distributive conditional type
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type NonNullable<T> = T extends null | undefined ? never : T;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

