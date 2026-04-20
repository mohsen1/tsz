#[test]
fn test_binder_variable_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let x = 1; const y = 2; var z = 3;".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that symbols were created
    assert!(binder.file_locals.has("x"));
    assert!(binder.file_locals.has("y"));
    assert!(binder.file_locals.has("z"));
}

#[test]
fn test_binder_reset_clears_state() {
    let mut parser = ParserState::new("test.ts".to_string(), "const a = 1;".to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(binder.file_locals.has("a"));
    assert!(!binder.symbols.is_empty());
    assert!(!binder.node_symbols.is_empty());

    binder.reset();

    assert!(binder.file_locals.is_empty());
    assert!(binder.symbols.is_empty());
    assert!(binder.node_symbols.is_empty());
    assert_eq!(binder.flow_nodes.len(), 1);

    let mut parser = ParserState::new("test.ts".to_string(), "const b = 2;".to_string());
    let root = parser.parse_source_file();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(binder.file_locals.has("b"));
    assert!(!binder.file_locals.has("a"));
}

#[test]
fn test_binder_function_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function foo(a: number, b: string) { return a; }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that function symbol was created
    assert!(binder.file_locals.has("foo"));
}

#[test]
fn test_binder_class_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class MyClass { x: number; foo() {} }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that class symbol was created
    assert!(binder.file_locals.has("MyClass"));
}

#[test]
fn test_binder_interface_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface IFoo { x: number; }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that interface symbol was created
    assert!(binder.file_locals.has("IFoo"));
}

#[test]
fn test_binder_type_alias() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type MyType = string | number;".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that type alias symbol was created
    assert!(binder.file_locals.has("MyType"));
}

#[test]
fn test_binder_enum_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "enum Color { Red, Green, Blue }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that enum symbol was created
    assert!(binder.file_locals.has("Color"));
}

// =============================================================================
// Import/Export Binding Tests
// =============================================================================

#[test]
fn test_binder_import_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"import foo from 'module'; import { bar, baz as qux } from 'other';"#.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Default import creates alias symbol
    assert!(binder.file_locals.has("foo"));
    // Named imports create alias symbols
    assert!(binder.file_locals.has("bar"));
    assert!(binder.file_locals.has("qux")); // aliased from baz
}

#[test]
fn test_binder_export_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"const x = 1; export { x, x as y };"#.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Variable should be bound
    assert!(binder.file_locals.has("x"));

    // Export specifiers should have symbols (marked via node_symbols, not file_locals)
    // This ensures the binding runs without errors
    assert!(
        binder.symbols.len() > 1,
        "Should have created export symbols"
    );
}

#[test]
fn test_binder_exported_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"export function foo() { return 1; }"#.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Exported function should be bound to file_locals
    assert!(
        binder.file_locals.has("foo"),
        "Exported function 'foo' should be in file_locals"
    );
}

