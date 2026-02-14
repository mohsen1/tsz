use super::*;
use tsz_parser::ParserState;

#[test]
fn test_symbols_api_simple() {
    let source = "function foo() {}\nconst x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].name, "foo");
    assert_eq!(tree[0].kind, SymbolKind::Function);
    assert_eq!(tree[1].name, "x");
    assert_eq!(tree[1].kind, SymbolKind::Constant);
}

#[test]
fn test_symbols_api_hierarchical() {
    let source = r#"
class MyClass {
    method1() {}
    property1: number;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "MyClass");
    assert_eq!(tree[0].kind, SymbolKind::Class);
    assert_eq!(tree[0].children.len(), 2);
    assert_eq!(tree[0].children[0].name, "method1");
    assert_eq!(tree[0].children[0].kind, SymbolKind::Method);
    assert_eq!(tree[0].children[1].name, "property1");
    assert_eq!(tree[0].children[1].kind, SymbolKind::Property);
}

#[test]
fn test_symbols_api_interface() {
    let source = r#"
interface Point {
    x: number;
    y: number;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Point");
    assert_eq!(tree[0].kind, SymbolKind::Interface);
    // Note: interface properties are not currently extracted as child symbols
}

#[test]
fn test_symbols_api_enum() {
    let source = "enum Color { Red, Green, Blue }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Color");
    assert_eq!(tree[0].kind, SymbolKind::Enum);
    assert_eq!(tree[0].children.len(), 3);
    assert_eq!(tree[0].children[0].name, "Red");
    assert_eq!(tree[0].children[0].kind, SymbolKind::EnumMember);
}

#[test]
fn test_symbols_api_namespace() {
    let source = r#"
namespace MyNamespace {
    function foo() {}
    const bar = 1;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "MyNamespace");
    assert_eq!(tree[0].kind, SymbolKind::Module);
    assert_eq!(tree[0].children.len(), 2); // foo and bar
}
