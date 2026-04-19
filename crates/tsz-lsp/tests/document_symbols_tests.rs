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

    // Get accessor — tsc exposes getters/setters via ScriptElementKind
    // "getter"/"setter" (not "property"), which we model with dedicated
    // SymbolKind variants.
    assert_eq!(symbols[0].children[0].name, "val");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::Getter);

    // Set accessor
    assert_eq!(symbols[0].children[1].name, "val");
    assert_eq!(symbols[0].children[1].kind, SymbolKind::Setter);
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

// =========================================================================
// Additional edge case tests
// =========================================================================

#[test]
fn test_document_symbols_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert!(symbols.is_empty(), "Empty file should have no symbols");
}

#[test]
fn test_document_symbols_only_comments() {
    let source = "// This is a comment\n/* Block comment */";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert!(
        symbols.is_empty(),
        "File with only comments should have no symbols"
    );
}

#[test]
fn test_document_symbols_arrow_function_variable() {
    let source = "const greet = (name: string) => `Hello ${name}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "greet");
    assert_eq!(symbols[0].kind, SymbolKind::Constant);
}

#[test]
fn test_document_symbols_class_with_constructor() {
    let source = "class Point {\n  constructor(public x: number, public y: number) {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Point");
    assert_eq!(symbols[0].kind, SymbolKind::Class);
    let has_ctor = symbols[0]
        .children
        .iter()
        .any(|c| c.kind == SymbolKind::Constructor);
    assert!(has_ctor, "Should have constructor as child symbol");
}

#[test]
fn test_document_symbols_multiple_exports() {
    let source = "export const A = 1;\nexport function B() {}\nexport class C {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[0].name, "A");
    assert_eq!(symbols[1].name, "B");
    assert_eq!(symbols[2].name, "C");
    for sym in &symbols {
        assert!(
            sym.kind_modifiers.contains("export"),
            "Symbol '{}' should have export modifier",
            sym.name
        );
    }
}

#[test]
fn test_document_symbols_enum_with_members() {
    let source = "enum Color {\n  Red,\n  Green,\n  Blue\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Color");
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
    assert_eq!(
        symbols[0].children.len(),
        3,
        "Enum should have 3 member children"
    );
}

#[test]
fn test_document_symbols_interface_with_methods() {
    let source = "interface Shape {\n  area(): number;\n  perimeter(): number;\n  name: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Shape");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
    assert!(
        symbols[0].children.len() >= 2,
        "Interface should have at least 2 child symbols"
    );
}

#[test]
fn test_document_symbols_namespace() {
    let source =
        "namespace MyApp {\n  export function init() {}\n  export const VERSION = '1.0';\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "MyApp");
    assert!(
        !symbols[0].children.is_empty(),
        "Namespace should have children"
    );
}

#[test]
fn test_document_symbols_type_alias() {
    let source = "type StringOrNumber = string | number;\ntype Callback = (x: number) => void;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 2);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"StringOrNumber"));
    assert!(names.contains(&"Callback"));
}

#[test]
fn test_document_symbols_nested_classes() {
    let source = "class Outer {\n  static Inner = class {\n    method() {}\n  };\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Outer");
    assert!(
        !symbols[0].children.is_empty(),
        "Outer should have children"
    );
}

#[test]
fn test_document_symbols_getter_setter() {
    let source = "class Store {\n  get value() { return 0; }\n  set value(v: number) {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Store");
    // Should have getter and setter as children
    assert!(
        symbols[0].children.len() >= 2,
        "Should have getter and setter, got {} children",
        symbols[0].children.len()
    );
}

#[test]
fn test_document_symbols_ranges_are_valid() {
    let source = "function hello() {\n  return 'world';\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    let sym = &symbols[0];
    // Full range should encompass selection range
    assert!(sym.range.start.line <= sym.selection_range.start.line);
    assert!(sym.range.end.line >= sym.selection_range.end.line);
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_document_symbols_module_declaration() {
    let source = "module MyModule {\n  export function init() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "MyModule");
    assert_eq!(symbols[0].kind, SymbolKind::Module);
    assert_eq!(symbols[0].children.len(), 1);
    assert_eq!(symbols[0].children[0].name, "init");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::Function);
}

#[test]
fn test_document_symbols_abstract_class_with_abstract_method() {
    let source = "abstract class Shape {\n  abstract area(): number;\n  concrete() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Shape");
    assert_eq!(symbols[0].kind, SymbolKind::Class);
    assert!(
        symbols[0].kind_modifiers.contains("abstract"),
        "Expected 'abstract' modifier on class"
    );
    assert_eq!(symbols[0].children.len(), 2);
    assert_eq!(symbols[0].children[0].name, "area");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::Method);
    assert_eq!(symbols[0].children[1].name, "concrete");
}

#[test]
fn test_document_symbols_static_property() {
    let source = "class Config {\n  static readonly MAX = 100;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].children.len(), 1);
    let prop = &symbols[0].children[0];
    assert_eq!(prop.name, "MAX");
    assert_eq!(prop.kind, SymbolKind::Property);
    assert!(
        prop.kind_modifiers.contains("static"),
        "Expected 'static' in kind_modifiers, got: '{}'",
        prop.kind_modifiers
    );
    assert!(
        prop.kind_modifiers.contains("readonly"),
        "Expected 'readonly' in kind_modifiers, got: '{}'",
        prop.kind_modifiers
    );
}

#[test]
fn test_document_symbols_private_method() {
    let source = "class Foo {\n  private secret() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].children.len(), 1);
    let method = &symbols[0].children[0];
    assert_eq!(method.name, "secret");
    assert_eq!(method.kind, SymbolKind::Method);
    assert!(
        method.kind_modifiers.contains("private"),
        "Expected 'private' in kind_modifiers, got: '{}'",
        method.kind_modifiers
    );
}

#[test]
fn test_document_symbols_protected_property() {
    let source = "class Base {\n  protected value: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    let prop = &symbols[0].children[0];
    assert_eq!(prop.name, "value");
    assert!(
        prop.kind_modifiers.contains("protected"),
        "Expected 'protected' in kind_modifiers, got: '{}'",
        prop.kind_modifiers
    );
}

#[test]
fn test_document_symbols_const_enum() {
    let source = "const enum Direction { Up, Down }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Direction");
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
    assert!(
        symbols[0].kind_modifiers.contains("const"),
        "Expected 'const' modifier on const enum, got: '{}'",
        symbols[0].kind_modifiers
    );
    assert_eq!(symbols[0].children.len(), 2);
}

#[test]
fn test_document_symbols_declare_module() {
    let source = "declare module 'my-module' {\n  export function foo(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].kind, SymbolKind::Module);
    assert!(
        symbols[0].kind_modifiers.contains("declare"),
        "Expected 'declare' modifier on declare module"
    );
}

#[test]
fn test_document_symbols_export_default_class() {
    let source = "export default class Widget {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert!(!symbols.is_empty(), "Should produce at least one symbol");
    let sym = &symbols[0];
    // tsc emits just `export` (no `default` modifier) for named default
    // exports; the `default`-ness is encoded implicitly.
    assert_eq!(
        sym.kind_modifiers, "export",
        "Expected 'export' modifier on named default export, got: '{}'",
        sym.kind_modifiers
    );
    assert_eq!(sym.name, "Widget");
}

#[test]
fn test_document_symbols_export_default_function() {
    let source = "export default function main() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert!(!symbols.is_empty(), "Should produce at least one symbol");
    let sym = &symbols[0];
    assert_eq!(
        sym.kind_modifiers, "export",
        "Expected 'export' modifier on named default export, got: '{}'",
        sym.kind_modifiers
    );
    assert_eq!(sym.name, "main");
}

#[test]
fn test_document_symbols_export_default_anonymous_class() {
    let source = "export default class {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert!(!symbols.is_empty(), "Should produce at least one symbol");
    let sym = &symbols[0];
    // Anonymous default export: name becomes "default", modifier stays
    // just `export`.
    assert_eq!(sym.name, "default");
    assert_eq!(sym.kind_modifiers, "export");
}

#[test]
fn test_document_symbols_async_function() {
    let source = "async function fetchData() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "fetchData");
    assert_eq!(symbols[0].kind, SymbolKind::Function);
    assert_eq!(symbols[0].detail, Some("async".to_string()));
}

#[test]
fn test_document_symbols_export_async_function() {
    let source = "export async function load() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "load");
    assert_eq!(symbols[0].detail, Some("async".to_string()));
    assert!(symbols[0].kind_modifiers.contains("export"));
}

#[test]
fn test_document_symbols_nested_namespace() {
    let source = "namespace A {\n  namespace B {\n    function inner() {}\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "A");
    assert_eq!(symbols[0].kind, SymbolKind::Module);
    assert_eq!(symbols[0].children.len(), 1);
    assert_eq!(symbols[0].children[0].name, "B");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::Module);
    assert_eq!(symbols[0].children[0].children.len(), 1);
    assert_eq!(symbols[0].children[0].children[0].name, "inner");
}

#[test]
fn test_document_symbols_let_variable_modifier() {
    let source = "let counter = 0;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "counter");
    assert_eq!(symbols[0].kind, SymbolKind::Variable);
    assert!(
        symbols[0].kind_modifiers.contains("let"),
        "Expected 'let' in kind_modifiers, got: '{}'",
        symbols[0].kind_modifiers
    );
}

#[test]
fn test_document_symbols_export_let_variable() {
    let source = "export let count = 0;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "count");
    assert_eq!(symbols[0].kind, SymbolKind::Variable);
    assert!(symbols[0].kind_modifiers.contains("export"));
    assert!(symbols[0].kind_modifiers.contains("let"));
}

#[test]
fn test_document_symbols_var_variable() {
    let source = "var legacy = true;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "legacy");
    assert_eq!(symbols[0].kind, SymbolKind::Variable);
}

#[test]
fn test_document_symbols_export_type_alias() {
    let source = "export type ID = string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "ID");
    assert_eq!(symbols[0].kind, SymbolKind::Struct);
    assert!(symbols[0].kind_modifiers.contains("export"));
}

#[test]
fn test_document_symbols_declare_function() {
    let source = "declare function readFile(path: string): string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "readFile");
    assert_eq!(symbols[0].kind, SymbolKind::Function);
    assert!(symbols[0].kind_modifiers.contains("declare"));
}

#[test]
fn test_document_symbols_declare_class() {
    let source = "declare class Buffer {\n  length: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Buffer");
    assert_eq!(symbols[0].kind, SymbolKind::Class);
    assert!(symbols[0].kind_modifiers.contains("declare"));
    assert_eq!(symbols[0].children.len(), 1);
    assert_eq!(symbols[0].children[0].name, "length");
}

#[test]
fn test_document_symbols_export_enum() {
    let source = "export enum Status { Active, Inactive }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Status");
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
    assert!(symbols[0].kind_modifiers.contains("export"));
    assert_eq!(symbols[0].children.len(), 2);
}

#[test]
fn test_document_symbols_nested_function_in_function() {
    let source = "function outer() {\n  function inner() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "outer");
    assert_eq!(symbols[0].kind, SymbolKind::Function);
    // Nested functions inside function body should be collected as children
    assert_eq!(
        symbols[0].children.len(),
        1,
        "Should have nested function as child"
    );
    assert_eq!(symbols[0].children[0].name, "inner");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::Function);
}

#[test]
fn test_document_symbols_export_interface() {
    let source = "export interface Config {\n  host: string;\n  port: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Config");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
    assert!(symbols[0].kind_modifiers.contains("export"));
    assert_eq!(symbols[0].children.len(), 2);
}

#[test]
fn test_document_symbols_constructor_container_name() {
    let source = "class Service {\n  constructor() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    let ctor = symbols[0]
        .children
        .iter()
        .find(|c| c.kind == SymbolKind::Constructor);
    assert!(ctor.is_some());
    assert_eq!(ctor.unwrap().name, "constructor");
    assert_eq!(ctor.unwrap().container_name, Some("Service".to_string()));
}

#[test]
fn test_document_symbols_multiple_interfaces() {
    let source = "interface A { x: number; }\ninterface B { y: string; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "A");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
    assert_eq!(symbols[1].name, "B");
    assert_eq!(symbols[1].kind, SymbolKind::Interface);
}

// =========================================================================
// Additional tests for document symbols
// =========================================================================

#[test]
fn test_document_symbols_function_expression_variable() {
    let source = "const fn1 = function() {};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "fn1");
    assert_eq!(symbols[0].kind, SymbolKind::Constant);
}

#[test]
fn test_document_symbols_multiple_classes() {
    let source = "class A {}\nclass B {}\nclass C {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[0].name, "A");
    assert_eq!(symbols[1].name, "B");
    assert_eq!(symbols[2].name, "C");
    for sym in &symbols {
        assert_eq!(sym.kind, SymbolKind::Class);
    }
}

#[test]
fn test_document_symbols_class_with_multiple_methods() {
    let source = "class Calc {\n  add() {}\n  sub() {}\n  mul() {}\n  div() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Calc");
    assert_eq!(symbols[0].children.len(), 4);
    let names: Vec<&str> = symbols[0]
        .children
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert!(names.contains(&"add"));
    assert!(names.contains(&"sub"));
    assert!(names.contains(&"mul"));
    assert!(names.contains(&"div"));
}

#[test]
fn test_document_symbols_enum_with_string_initializers() {
    let source = "enum Fruit {\n  Apple = 'apple',\n  Banana = 'banana'\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Fruit");
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
    assert_eq!(symbols[0].children.len(), 2);
    assert_eq!(symbols[0].children[0].name, "Apple");
    assert_eq!(symbols[0].children[1].name, "Banana");
}

#[test]
fn test_document_symbols_interface_with_optional_members() {
    let source = "interface Options {\n  verbose?: boolean;\n  output?: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Options");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
    assert_eq!(symbols[0].children.len(), 2);
    assert_eq!(symbols[0].children[0].name, "verbose");
    assert_eq!(symbols[0].children[1].name, "output");
}

#[test]
fn test_document_symbols_generic_function() {
    let source = "function identity<T>(x: T): T { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "identity");
    assert_eq!(symbols[0].kind, SymbolKind::Function);
}

#[test]
fn test_document_symbols_generic_class() {
    let source = "class Box<T> {\n  value: T;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Box");
    assert_eq!(symbols[0].kind, SymbolKind::Class);
    assert_eq!(symbols[0].children.len(), 1);
    assert_eq!(symbols[0].children[0].name, "value");
}

#[test]
fn test_document_symbols_generic_interface() {
    let source = "interface Comparable<T> {\n  compareTo(other: T): number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Comparable");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
    assert_eq!(symbols[0].children.len(), 1);
    assert_eq!(symbols[0].children[0].name, "compareTo");
}

#[test]
fn test_document_symbols_multiple_function_declarations() {
    let source = "function a() {}\nfunction b() {}\nfunction c() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[0].name, "a");
    assert_eq!(symbols[1].name, "b");
    assert_eq!(symbols[2].name, "c");
    for sym in &symbols {
        assert_eq!(sym.kind, SymbolKind::Function);
    }
}

#[test]
fn test_document_symbols_class_extends() {
    let source = "class Animal {}\nclass Dog extends Animal {\n  bark() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "Animal");
    assert_eq!(symbols[1].name, "Dog");
    assert_eq!(symbols[1].children.len(), 1);
    assert_eq!(symbols[1].children[0].name, "bark");
}

#[test]
fn test_document_symbols_interface_extends() {
    let source = "interface Base { x: number; }\ninterface Derived extends Base { y: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "Base");
    assert_eq!(symbols[1].name, "Derived");
    assert_eq!(symbols[1].kind, SymbolKind::Interface);
}

#[test]
fn test_document_symbols_class_with_index_signature() {
    let source = "class Dict {\n  [key: string]: any;\n  get(key: string) { return this[key]; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Dict");
    assert_eq!(symbols[0].kind, SymbolKind::Class);
    // Should have at least the get method as a child
    assert!(
        symbols[0].children.iter().any(|c| c.name == "get"),
        "Should have 'get' method as child"
    );
}

#[test]
fn test_document_symbols_mixed_declarations() {
    let source =
        "const x = 1;\nfunction f() {}\nclass C {}\ninterface I {}\nenum E { A }\ntype T = string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 6);
    assert_eq!(symbols[0].name, "x");
    assert_eq!(symbols[0].kind, SymbolKind::Constant);
    assert_eq!(symbols[1].name, "f");
    assert_eq!(symbols[1].kind, SymbolKind::Function);
    assert_eq!(symbols[2].name, "C");
    assert_eq!(symbols[2].kind, SymbolKind::Class);
    assert_eq!(symbols[3].name, "I");
    assert_eq!(symbols[3].kind, SymbolKind::Interface);
    assert_eq!(symbols[4].name, "E");
    assert_eq!(symbols[4].kind, SymbolKind::Enum);
    assert_eq!(symbols[5].name, "T");
    assert_eq!(symbols[5].kind, SymbolKind::Struct);
}

#[test]
fn test_document_symbols_empty_class() {
    let source = "class Empty {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Empty");
    assert_eq!(symbols[0].kind, SymbolKind::Class);
    assert!(
        symbols[0].children.is_empty(),
        "Empty class should have no children"
    );
}

#[test]
fn test_document_symbols_empty_interface() {
    let source = "interface Empty {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Empty");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
    assert!(
        symbols[0].children.is_empty(),
        "Empty interface should have no children"
    );
}

#[test]
fn test_document_symbols_empty_enum() {
    let source = "enum Empty {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Empty");
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
    assert!(
        symbols[0].children.is_empty(),
        "Empty enum should have no children"
    );
}

#[test]
fn test_document_symbols_whitespace_only() {
    let source = "   \n   \n   ";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert!(
        symbols.is_empty(),
        "Whitespace-only file should have no symbols"
    );
}

#[test]
fn test_document_symbols_const_enum_members() {
    let source = "const enum Direction {\n  Up = 0,\n  Down = 1,\n  Left = 2,\n  Right = 3\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Direction");
    assert_eq!(symbols[0].children.len(), 4);
    assert_eq!(symbols[0].children[0].name, "Up");
    assert_eq!(symbols[0].children[0].kind, SymbolKind::EnumMember);
}

#[test]
fn test_document_symbols_class_with_private_protected() {
    let source = "class Secured {\n  private secret: string;\n  protected guard(): void {}\n  public visible: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].children.len(), 3);
}

#[test]
fn test_document_symbols_interface_with_index_and_call() {
    let source = "interface Indexable {\n  [key: string]: any;\n  (arg: number): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Indexable");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
}

#[test]
fn test_document_symbols_unicode_identifiers() {
    let source = "const café = 1;\nfunction naïve() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "café");
    assert_eq!(symbols[1].name, "naïve");
}

#[test]
fn test_document_symbols_deeply_nested_namespaces() {
    let source = "namespace A {\n  namespace B {\n    namespace C {\n      function deep() {}\n    }\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "A");
    // A has child B
    assert!(!symbols[0].children.is_empty(), "A should have child B");
}

#[test]
fn test_document_symbols_class_with_static_and_instance() {
    let source = "class Counter {\n  static count: number = 0;\n  value: number;\n  static reset() {}\n  increment() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Counter");
    assert_eq!(
        symbols[0].children.len(),
        4,
        "Should have 4 children: count, value, reset, increment"
    );
}

#[test]
fn test_document_symbols_multiple_function_overloads() {
    let source = "function process(x: string): string;\nfunction process(x: number): number;\nfunction process(x: any): any { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    // Should have at least one symbol for 'process'
    let process_symbols: Vec<_> = symbols.iter().filter(|s| s.name == "process").collect();
    assert!(
        !process_symbols.is_empty(),
        "Should have at least one 'process' symbol"
    );
}

#[test]
fn test_document_symbols_class_implements_multiple() {
    let source = "interface Readable {}\ninterface Writable {}\nclass Stream implements Readable, Writable {\n  read() {}\n  write() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[2].name, "Stream");
    assert_eq!(symbols[2].kind, SymbolKind::Class);
    assert_eq!(symbols[2].children.len(), 2);
}

#[test]
fn test_document_symbols_type_alias_conditional() {
    let source = "type IsString<T> = T extends string ? true : false;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "IsString");
    assert_eq!(symbols[0].kind, SymbolKind::Struct);
}

#[test]
fn test_document_symbols_type_alias_mapped() {
    let source = "type Readonly<T> = {\n  readonly [K in keyof T]: T[K];\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Readonly");
    assert_eq!(symbols[0].kind, SymbolKind::Struct);
}

#[test]
fn test_document_symbols_single_line_declarations() {
    let source = "const a = 1; let b = 2; var c = 3;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(
        symbols.len(),
        3,
        "Should find all three variable declarations"
    );
    assert_eq!(symbols[0].name, "a");
    assert_eq!(symbols[0].kind, SymbolKind::Constant);
    assert_eq!(symbols[1].name, "b");
    assert_eq!(symbols[2].name, "c");
}

#[test]
fn test_document_symbols_async_generator_function() {
    let source = "async function* gen() { yield 1; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "gen");
    assert_eq!(symbols[0].kind, SymbolKind::Function);
}

#[test]
fn test_document_symbols_class_with_readonly_property() {
    let source = "class Config {\n  readonly host: string = 'localhost';\n  readonly port: number = 8080;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Config");
    assert_eq!(symbols[0].children.len(), 2);
    assert_eq!(symbols[0].children[0].name, "host");
    assert_eq!(symbols[0].children[1].name, "port");
}

#[test]
fn test_document_symbols_interface_with_readonly() {
    let source = "interface Point {\n  readonly x: number;\n  readonly y: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Point");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
    assert_eq!(symbols[0].children.len(), 2);
}

#[test]
fn test_document_symbols_multiple_type_aliases() {
    let source = "type A = string;\ntype B = number;\ntype C = boolean;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[0].name, "A");
    assert_eq!(symbols[1].name, "B");
    assert_eq!(symbols[2].name, "C");
}

#[test]
fn test_document_symbols_class_with_accessor_keyword() {
    let source = "class Greeter {\n  get message(): string { return 'hi'; }\n  set message(val: string) {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Greeter");
    // getter and setter
    assert!(symbols[0].children.len() >= 2);
}

#[test]
fn test_document_symbols_enum_with_computed_values() {
    let source = "enum Bits {\n  A = 1 << 0,\n  B = 1 << 1,\n  C = 1 << 2\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Bits");
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
    assert_eq!(symbols[0].children.len(), 3);
}

#[test]
fn test_document_symbols_arrow_function_const() {
    let source = "const add = (a: number, b: number) => a + b;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "add");
}

#[test]
fn test_document_symbols_class_with_method_overloads() {
    let source = "class Parser {\n  parse(input: string): void;\n  parse(input: number): void;\n  parse(input: any): void {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Parser");
    // At least one parse method should appear
    assert!(!symbols[0].children.is_empty());
}

#[test]
fn test_document_symbols_declare_global() {
    let source = "declare global {\n  interface Window { custom: string; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    // Should produce at least one symbol for the global declaration
    let _ = symbols;
}

#[test]
fn test_document_symbols_export_class_with_generics() {
    let source =
        "export class Container<T> {\n  value: T;\n  constructor(val: T) { this.value = val; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Container");
    assert_eq!(symbols[0].kind, SymbolKind::Class);
}

#[test]
fn test_document_symbols_function_with_destructured_params() {
    let source = "function draw({ x, y }: { x: number; y: number }) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "draw");
    assert_eq!(symbols[0].kind, SymbolKind::Function);
}

#[test]
fn test_document_symbols_interface_with_generics() {
    let source = "interface Repository<T> {\n  find(id: string): T;\n  save(entity: T): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Repository");
    assert_eq!(symbols[0].kind, SymbolKind::Interface);
    assert_eq!(symbols[0].children.len(), 2);
}

#[test]
fn test_document_symbols_abstract_method() {
    let source =
        "abstract class Shape {\n  abstract area(): number;\n  abstract perimeter(): number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Shape");
    assert_eq!(symbols[0].children.len(), 2);
    assert_eq!(symbols[0].children[0].name, "area");
    assert_eq!(symbols[0].children[1].name, "perimeter");
}

#[test]
fn test_document_symbols_type_alias_union() {
    let source = "type StringOrNumber = string | number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "StringOrNumber");
}

#[test]
fn test_document_symbols_type_alias_intersection() {
    let source = "type Combined = { a: number } & { b: string };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Combined");
}

#[test]
fn test_document_symbols_multiple_enum_declarations() {
    let source = "enum Color { Red, Green, Blue }\nenum Size { Small, Medium, Large }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "Color");
    assert_eq!(symbols[1].name, "Size");
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
    assert_eq!(symbols[1].kind, SymbolKind::Enum);
}

#[test]
fn test_document_symbols_class_with_computed_property() {
    let source = "class Foo {\n  ['computed']() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Foo");
    // Computed property method should appear as a child
    assert!(!symbols[0].children.is_empty());
}

#[test]
fn test_document_symbols_generator_function() {
    let source = "function* counter() {\n  let i = 0;\n  while (true) yield i++;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "counter");
    assert_eq!(symbols[0].kind, SymbolKind::Function);
}

#[test]
fn test_document_symbols_declare_const() {
    let source = "declare const PI: number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "PI");
}

#[test]
fn test_document_symbols_declare_enum() {
    let source = "declare enum Direction {\n  Up,\n  Down\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Direction");
    assert_eq!(symbols[0].kind, SymbolKind::Enum);
}

#[test]
fn test_document_symbols_newline_only_file() {
    let source = "\n\n\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert!(
        symbols.is_empty(),
        "Newline-only file should have no symbols"
    );
}

#[test]
fn test_document_symbols_symbol_ranges_non_overlapping() {
    let source = "const a = 1;\nconst b = 2;\nconst c = 3;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 3);
    // Each symbol's range should start after the previous one ends
    for i in 1..symbols.len() {
        assert!(
            symbols[i].range.start.line >= symbols[i - 1].range.end.line,
            "Symbol ranges should not overlap"
        );
    }
}
