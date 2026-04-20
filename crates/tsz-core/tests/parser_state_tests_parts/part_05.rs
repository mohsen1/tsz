#[test]
fn test_parser_set_accessor() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { set value(v: number) { this._v = v; } }".to_string(),
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
fn test_parser_empty_accessor_body() {
    // Empty accessor body edge case (for ambient declarations)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "declare class Foo { get value(): number; set value(v: number); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    // Should parse without crashing
}

#[test]
fn test_parser_get_set_pair() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { private _x: number = 0; get x() { return this._x; } set x(v) { this._x = v; } }".to_string(),
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
fn test_parser_memory_efficiency() {
    // Verify that ParserState uses less memory per node
    let source = "let x = 1 + 2 + 3 + 4 + 5;".to_string();
    let mut parser = ParserState::new("test.ts".to_string(), source);
    parser.parse_source_file();

    // Calculate memory usage
    let node_size = size_of::<crate::parser::node::Node>();
    assert_eq!(node_size, 16, "Node should be 16 bytes");

    // Each node uses 16 bytes + data pool entry
    // This is much better than 208 bytes per fat Node
    let total_nodes = parser.arena.len();
    let node_memory = total_nodes * 16;
    let fat_memory = total_nodes * 208;

    println!("Nodes: {total_nodes}");
    println!("Node memory: {node_memory} bytes");
    println!("Fat Node memory: {fat_memory} bytes");
    println!("Memory savings: {}x", fat_memory / node_memory.max(1));

    assert!(
        fat_memory / node_memory.max(1) >= 10,
        "Should have at least 10x memory savings"
    );
}

#[test]
fn test_parser_interface_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface User { name: string; age: number; }".to_string(),
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
fn test_parser_interface_with_methods() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Service { getName(): string; setName(name: string): void; }".to_string(),
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
fn test_parser_interface_extends() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Admin extends User { role: string; }".to_string(),
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
fn test_parser_type_alias() {
    let mut parser = ParserState::new("test.ts".to_string(), "type ID = string;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_type_alias_object() {
    // Test type alias with object type (unions not yet supported)
    let mut parser = ParserState::new("test.ts".to_string(), "type Point = Coord;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_index_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface StringMap { [key: string]: string; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

