use super::*;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

#[test]
fn test_document_symbols_class_with_members() {
    let source = "class Foo {\n  bar() {}\n  prop: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Foo");
    assert_eq!(symbols[0].kind, SymbolKind::Class);
    assert_eq!(symbols[0].children.len(), 2); // bar, prop

    assert_eq!(symbols[0].children[0].name, "bar");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::Method);

    assert_eq!(symbols[0].children[1].name, "prop");
    assert_eq!(symbols[0].children[1].kind, SymbolKind::Property);
}

#[test]
fn test_document_symbols_function_and_variable() {
    let source = "function baz() {}\nconst x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 2);

    assert_eq!(symbols[0].name, "baz");
    assert_eq!(symbols[0].kind, SymbolKind::Function);

    assert_eq!(symbols[1].name, "x");
    assert_eq!(symbols[1].kind, SymbolKind::Constant);
}

#[test]
fn test_document_symbols_interface() {
    let source = "interface Point {\n  x: number;\n  y: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Point");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
}

#[test]
fn test_document_symbols_enum() {
    let source = "enum Color { Red, Green, Blue }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Color");
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
    assert_eq!(symbols[0].children.len(), 3);

    assert_eq!(symbols[0].children[0].name, "Red");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::EnumMember);
}

#[test]
fn test_document_symbols_multiple_variables() {
    let source = "const a = 1, b = 2;\nlet c = 3;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    // Should have 3 symbols: a (const), b (const), c (var)
    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[0].name, "a");
    assert_eq!(symbols[0].kind, SymbolKind::Constant);
    assert_eq!(symbols[1].name, "b");
    assert_eq!(symbols[1].kind, SymbolKind::Constant);
    assert_eq!(symbols[2].name, "c");
    assert_eq!(symbols[2].kind, SymbolKind::Variable);
}

// ============================================================
// New tests for enhanced document symbol features
// ============================================================

#[test]
fn test_kind_modifiers_export() {
    let source = "export function greet() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "greet");
    assert_eq!(symbols[0].kind, SymbolKind::Function);
    assert!(
        symbols[0].kind_modifiers.contains("export"),
        "Expected 'export' in kind_modifiers, got: '{}'",
        symbols[0].kind_modifiers
    );
}

#[test]
fn test_kind_modifiers_declare() {
    let source = "declare function nativeFn(): void;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "nativeFn");
    assert!(
        symbols[0].kind_modifiers.contains("declare"),
        "Expected 'declare' in kind_modifiers, got: '{}'",
        symbols[0].kind_modifiers
    );
}

#[test]
fn test_kind_modifiers_abstract_class() {
    let source = "export abstract class Base {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Base");
    assert_eq!(symbols[0].kind, SymbolKind::Class);
    assert!(
        symbols[0].kind_modifiers.contains("export"),
        "Expected 'export' in kind_modifiers, got: '{}'",
        symbols[0].kind_modifiers
    );
    assert!(
        symbols[0].kind_modifiers.contains("abstract"),
        "Expected 'abstract' in kind_modifiers, got: '{}'",
        symbols[0].kind_modifiers
    );
}

#[test]
fn test_kind_modifiers_static_method() {
    let source = "class Foo {\n  static bar() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].children.len(), 1);
    assert_eq!(symbols[0].children[0].name, "bar");
    assert!(
        symbols[0].children[0].kind_modifiers.contains("static"),
        "Expected 'static' in kind_modifiers, got: '{}'",
        symbols[0].children[0].kind_modifiers
    );
}

#[test]
fn test_container_name_for_class_members() {
    let source = "class MyClass {\n  myMethod() {}\n  myProp: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].container_name, None); // top-level has no container
    assert_eq!(
        symbols[0].children[0].container_name,
        Some("MyClass".to_string())
    );
    assert_eq!(
        symbols[0].children[1].container_name,
        Some("MyClass".to_string())
    );
}

#[test]
fn test_name_span_separate_from_range() {
    let source = "function hello() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    // The range should encompass the entire function
    // The selection_range should be just the identifier "hello"
    assert!(
        symbols[0].range.start.character <= symbols[0].selection_range.start.character,
        "range.start should be <= selection_range.start"
    );
    assert!(
        symbols[0].range.end.character >= symbols[0].selection_range.end.character
            || symbols[0].range.end.line > symbols[0].selection_range.end.line,
        "range.end should be >= selection_range.end"
    );
    // selection_range should be narrower
    let sel_width =
        symbols[0].selection_range.end.character - symbols[0].selection_range.start.character;
    assert_eq!(
        sel_width, 5,
        "selection_range width should be 5 for 'hello'"
    );
}

#[test]
fn test_enum_members_as_children() {
    let source = "enum Direction { Up, Down, Left, Right }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
    assert_eq!(symbols[0].children.len(), 4);
    assert_eq!(symbols[0].children[0].name, "Up");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::EnumMember);
    assert_eq!(
        symbols[0].children[0].container_name,
        Some("Direction".to_string())
    );
    assert_eq!(symbols[0].children[3].name, "Right");
}

#[test]
fn test_namespace_with_children() {
    let source = "namespace Utils {\n  function helper() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Utils");
    assert_eq!(symbols[0].kind, SymbolKind::Module);
    assert_eq!(symbols[0].children.len(), 1);
    assert_eq!(symbols[0].children[0].name, "helper");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::Function);
    assert_eq!(
        symbols[0].children[0].container_name,
        Some("Utils".to_string())
    );
}

#[test]
fn test_export_default_expression() {
    let source = "export default 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "default");
    assert_eq!(symbols[0].kind, SymbolKind::Variable);
}

#[test]
fn test_get_set_accessors() {
    let source = "class Obj {\n  get val() { return 1; }\n  set val(v: number) {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].children.len(), 2);

    // Get accessor
    assert_eq!(symbols[0].children[0].name, "val");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::Property);
    assert_eq!(symbols[0].children[0].detail, Some("getter".to_string()));
    assert!(symbols[0].children[0].kind_modifiers.contains("getter"));

    // Set accessor
    assert_eq!(symbols[0].children[1].name, "val");
    assert_eq!(symbols[0].children[1].kind, SymbolKind::Property);
    assert_eq!(symbols[0].children[1].detail, Some("setter".to_string()));
    assert!(symbols[0].children[1].kind_modifiers.contains("setter"));
}

#[test]
fn test_interface_members() {
    let source = "interface IFoo {\n  x: number;\n  doStuff(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "IFoo");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
    assert_eq!(symbols[0].children.len(), 2);

    assert_eq!(symbols[0].children[0].name, "x");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::Property);
    assert_eq!(
        symbols[0].children[0].container_name,
        Some("IFoo".to_string())
    );

    assert_eq!(symbols[0].children[1].name, "doStuff");
    assert_eq!(symbols[0].children[1].kind, SymbolKind::Method);
}

#[test]
fn test_export_const_variable() {
    let source = "export const MAX = 100;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "MAX");
    assert_eq!(symbols[0].kind, SymbolKind::Constant);
    assert!(
        symbols[0].kind_modifiers.contains("export"),
        "Expected 'export' in kind_modifiers, got: '{}'",
        symbols[0].kind_modifiers
    );
}

#[test]
fn test_type_alias() {
    let source = "type Point = { x: number; y: number };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Point");
    // Type aliases use SymbolKind::Struct which maps to "type" in tsserver.
    // TypeParameter is reserved for generic type params like <T>.
    assert_eq!(symbols[0].kind, SymbolKind::Struct);
}

#[test]
fn test_to_script_element_kind() {
    assert_eq!(SymbolKind::File.to_script_element_kind(), "script");
    assert_eq!(SymbolKind::Module.to_script_element_kind(), "module");
    assert_eq!(SymbolKind::Class.to_script_element_kind(), "class");
    assert_eq!(SymbolKind::Interface.to_script_element_kind(), "interface");
    assert_eq!(SymbolKind::Function.to_script_element_kind(), "function");
    assert_eq!(SymbolKind::Variable.to_script_element_kind(), "var");
    assert_eq!(SymbolKind::Constant.to_script_element_kind(), "const");
    assert_eq!(SymbolKind::Enum.to_script_element_kind(), "enum");
    assert_eq!(
        SymbolKind::EnumMember.to_script_element_kind(),
        "enum member"
    );
    assert_eq!(SymbolKind::Method.to_script_element_kind(), "method");
    assert_eq!(SymbolKind::Property.to_script_element_kind(), "property");
    assert_eq!(
        SymbolKind::Constructor.to_script_element_kind(),
        "constructor"
    );
    assert_eq!(
        SymbolKind::TypeParameter.to_script_element_kind(),
        "type parameter"
    );
}
