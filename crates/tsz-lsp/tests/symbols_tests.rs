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

#[test]
fn test_symbols_api_type_alias_union() {
    let source = "type StringOrNumber = string | number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "StringOrNumber");
    assert_eq!(tree[0].kind, SymbolKind::Struct);
    assert!(tree[0].children.is_empty());
}

#[test]
fn test_symbols_api_type_alias_generic() {
    let source = "type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Result");
    assert_eq!(tree[0].kind, SymbolKind::Struct);
}

#[test]
fn test_symbols_api_nested_class_in_namespace() {
    let source = r#"
namespace Outer {
    class Inner {
        method() {}
        prop: string;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Outer");
    assert_eq!(tree[0].kind, SymbolKind::Module);
    assert_eq!(tree[0].children.len(), 1);
    assert_eq!(tree[0].children[0].name, "Inner");
    assert_eq!(tree[0].children[0].kind, SymbolKind::Class);
    assert_eq!(tree[0].children[0].children.len(), 2);
    assert_eq!(tree[0].children[0].children[0].name, "method");
    assert_eq!(tree[0].children[0].children[0].kind, SymbolKind::Method);
    assert_eq!(tree[0].children[0].children[1].name, "prop");
    assert_eq!(tree[0].children[0].children[1].kind, SymbolKind::Property);
}

#[test]
fn test_symbols_api_multiple_functions_and_variables() {
    let source = "function a() {}\nfunction b() {}\nconst c = 1;\nlet d = 2;\nvar e = 3;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 5);
    assert_eq!(tree[0].name, "a");
    assert_eq!(tree[0].kind, SymbolKind::Function);
    assert_eq!(tree[1].name, "b");
    assert_eq!(tree[1].kind, SymbolKind::Function);
    assert_eq!(tree[2].name, "c");
    assert_eq!(tree[2].kind, SymbolKind::Constant);
    assert_eq!(tree[3].name, "d");
    assert_eq!(tree[3].kind, SymbolKind::Variable);
    assert_eq!(tree[4].name, "e");
    assert_eq!(tree[4].kind, SymbolKind::Variable);
}

#[test]
fn test_symbols_api_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert!(tree.is_empty(), "Empty file should produce no symbols");
}

#[test]
fn test_symbols_api_only_comments() {
    let source = "// this is a comment\n/* block comment */";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert!(
        tree.is_empty(),
        "File with only comments should produce no symbols"
    );
}

#[test]
fn test_symbols_api_getters_and_setters() {
    let source = r#"
class Config {
    get value() { return 1; }
    set value(v: number) {}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Config");
    assert_eq!(tree[0].kind, SymbolKind::Class);
    assert_eq!(tree[0].children.len(), 2);
    // Getter
    assert_eq!(tree[0].children[0].name, "value");
    assert_eq!(tree[0].children[0].kind, SymbolKind::Property);
    // Setter
    assert_eq!(tree[0].children[1].name, "value");
    assert_eq!(tree[0].children[1].kind, SymbolKind::Property);
}

#[test]
fn test_symbols_api_static_methods_and_properties() {
    let source = r#"
class MathUtils {
    static PI: number;
    static add(a: number, b: number) {}
    instance_method() {}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].children.len(), 3);
    assert_eq!(tree[0].children[0].name, "PI");
    assert_eq!(tree[0].children[0].kind, SymbolKind::Property);
    assert_eq!(tree[0].children[1].name, "add");
    assert_eq!(tree[0].children[1].kind, SymbolKind::Method);
    assert_eq!(tree[0].children[2].name, "instance_method");
    assert_eq!(tree[0].children[2].kind, SymbolKind::Method);
}

#[test]
fn test_symbols_api_abstract_class() {
    let source = r#"
abstract class Shape {
    abstract area(): number;
    name: string;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Shape");
    assert_eq!(tree[0].kind, SymbolKind::Class);
    assert_eq!(tree[0].children.len(), 2);
    assert_eq!(tree[0].children[0].name, "area");
    assert_eq!(tree[0].children[0].kind, SymbolKind::Method);
    assert_eq!(tree[0].children[1].name, "name");
    assert_eq!(tree[0].children[1].kind, SymbolKind::Property);
}

#[test]
fn test_symbols_api_export_class_and_interface() {
    let source = "export class Foo {}\nexport interface Bar { x: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].name, "Foo");
    assert_eq!(tree[0].kind, SymbolKind::Class);
    assert_eq!(tree[1].name, "Bar");
    assert_eq!(tree[1].kind, SymbolKind::Interface);
}

#[test]
fn test_symbols_api_constructor() {
    let source = r#"
class Service {
    constructor(private name: string) {}
    run() {}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Service");
    assert_eq!(tree[0].kind, SymbolKind::Class);
    // Should have constructor and run
    let has_constructor = tree[0]
        .children
        .iter()
        .any(|c| c.name == "constructor" && c.kind == SymbolKind::Constructor);
    assert!(has_constructor, "Should find constructor symbol");
    let has_run = tree[0]
        .children
        .iter()
        .any(|c| c.name == "run" && c.kind == SymbolKind::Method);
    assert!(has_run, "Should find run method symbol");
}

#[test]
fn test_symbols_api_interface_with_method_signatures() {
    let source = r#"
interface Serializable {
    serialize(): string;
    deserialize(data: string): void;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Serializable");
    assert_eq!(tree[0].kind, SymbolKind::Interface);
    assert_eq!(tree[0].children.len(), 2);
    assert_eq!(tree[0].children[0].name, "serialize");
    assert_eq!(tree[0].children[0].kind, SymbolKind::Method);
    assert_eq!(tree[0].children[1].name, "deserialize");
    assert_eq!(tree[0].children[1].kind, SymbolKind::Method);
}

#[test]
fn test_symbols_api_multiple_types_mixed() {
    let source = r#"
type ID = string;
interface User { id: ID; name: string; }
class UserService {
    getUser(): User { return { id: "1", name: "test" }; }
}
enum Role { Admin, User }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 4);
    assert_eq!(tree[0].name, "ID");
    assert_eq!(tree[0].kind, SymbolKind::Struct);
    assert_eq!(tree[1].name, "User");
    assert_eq!(tree[1].kind, SymbolKind::Interface);
    assert_eq!(tree[2].name, "UserService");
    assert_eq!(tree[2].kind, SymbolKind::Class);
    assert_eq!(tree[3].name, "Role");
    assert_eq!(tree[3].kind, SymbolKind::Enum);
}

#[test]
fn test_symbols_api_export_default_function() {
    let source = "export default function main() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    // Should produce a symbol for the function
    assert!(!tree.is_empty(), "Should produce at least one symbol");
    // The function name should be "main"
    let has_main = tree.iter().any(|s| s.name == "main");
    // Or it might be "default" depending on how export default is handled
    let has_default = tree.iter().any(|s| s.name == "default");
    assert!(
        has_main || has_default,
        "Should have either 'main' or 'default' symbol, got: {:?}",
        tree.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_symbols_api_type_alias_intersection() {
    let source = "type Named = { name: string } & { id: number };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Named");
    assert_eq!(tree[0].kind, SymbolKind::Struct);
    assert!(tree[0].children.is_empty());
}

#[test]
fn test_symbols_api_type_alias_mapped() {
    let source = "type Readonly<T> = { readonly [K in keyof T]: T[K] };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Readonly");
    assert_eq!(tree[0].kind, SymbolKind::Struct);
}

#[test]
fn test_symbols_api_type_alias_conditional() {
    let source = "type NonNullable<T> = T extends null | undefined ? never : T;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "NonNullable");
    assert_eq!(tree[0].kind, SymbolKind::Struct);
}

#[test]
fn test_symbols_api_nested_namespaces_with_classes() {
    let source = r#"
namespace Outer {
    namespace Inner {
        class Widget {
            render() {}
        }
    }
    class Container {
        add() {}
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Outer");
    assert_eq!(tree[0].kind, SymbolKind::Module);
    assert_eq!(tree[0].children.len(), 2); // Inner namespace and Container class

    // Find Inner namespace
    let inner = tree[0].children.iter().find(|c| c.name == "Inner");
    assert!(inner.is_some(), "Should have Inner namespace");
    let inner = inner.unwrap();
    assert_eq!(inner.kind, SymbolKind::Module);
    assert_eq!(inner.children.len(), 1);
    assert_eq!(inner.children[0].name, "Widget");
    assert_eq!(inner.children[0].kind, SymbolKind::Class);
    assert_eq!(inner.children[0].children.len(), 1);
    assert_eq!(inner.children[0].children[0].name, "render");

    // Find Container class
    let container = tree[0].children.iter().find(|c| c.name == "Container");
    assert!(container.is_some(), "Should have Container class");
    let container = container.unwrap();
    assert_eq!(container.kind, SymbolKind::Class);
}

#[test]
fn test_symbols_api_arrow_function_const() {
    // Arrow functions assigned to const should show up as Constant symbols
    let source = "const greet = (name: string) => `Hello ${name}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "greet");
    assert_eq!(tree[0].kind, SymbolKind::Constant);
}

#[test]
fn test_symbols_api_arrow_function_let() {
    // Arrow functions assigned to let should show up as Variable symbols
    let source = "let handler = () => {};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "handler");
    assert_eq!(tree[0].kind, SymbolKind::Variable);
}

#[test]
fn test_symbols_api_export_class_with_members() {
    let source = r#"
export class Service {
    private url: string;
    async fetch() {}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Service");
    assert_eq!(tree[0].kind, SymbolKind::Class);
    assert_eq!(tree[0].children.len(), 2);
    assert_eq!(tree[0].children[0].name, "url");
    assert_eq!(tree[0].children[0].kind, SymbolKind::Property);
    assert_eq!(tree[0].children[1].name, "fetch");
    assert_eq!(tree[0].children[1].kind, SymbolKind::Method);
}

#[test]
fn test_symbols_api_export_interface_with_members() {
    let source = "export interface Config { host: string; port: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Config");
    assert_eq!(tree[0].kind, SymbolKind::Interface);
    assert_eq!(tree[0].children.len(), 2);
    assert_eq!(tree[0].children[0].name, "host");
    assert_eq!(tree[0].children[1].name, "port");
}

#[test]
fn test_symbols_api_abstract_class_with_mixed_members() {
    let source = r#"
abstract class Animal {
    abstract makeSound(): string;
    move(distance: number) {}
    name: string;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Animal");
    assert_eq!(tree[0].kind, SymbolKind::Class);
    assert_eq!(tree[0].children.len(), 3);

    let make_sound = &tree[0].children[0];
    assert_eq!(make_sound.name, "makeSound");
    assert_eq!(make_sound.kind, SymbolKind::Method);

    let move_method = &tree[0].children[1];
    assert_eq!(move_method.name, "move");
    assert_eq!(move_method.kind, SymbolKind::Method);

    let name_prop = &tree[0].children[2];
    assert_eq!(name_prop.name, "name");
    assert_eq!(name_prop.kind, SymbolKind::Property);
}

#[test]
fn test_symbols_api_static_and_instance_mixed() {
    let source = r#"
class Counter {
    static count: number;
    static increment() {}
    value: number;
    reset() {}
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Counter");
    assert_eq!(tree[0].children.len(), 4);

    assert_eq!(tree[0].children[0].name, "count");
    assert_eq!(tree[0].children[0].kind, SymbolKind::Property);
    assert_eq!(tree[0].children[1].name, "increment");
    assert_eq!(tree[0].children[1].kind, SymbolKind::Method);
    assert_eq!(tree[0].children[2].name, "value");
    assert_eq!(tree[0].children[2].kind, SymbolKind::Property);
    assert_eq!(tree[0].children[3].name, "reset");
    assert_eq!(tree[0].children[3].kind, SymbolKind::Method);
}

#[test]
fn test_symbols_api_getters_setters_with_different_names() {
    let source = r#"
class Box {
    get width() { return 0; }
    set width(w: number) {}
    get height() { return 0; }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Box");
    assert_eq!(tree[0].children.len(), 3);

    assert_eq!(tree[0].children[0].name, "width");
    assert_eq!(tree[0].children[0].kind, SymbolKind::Property);
    assert_eq!(tree[0].children[1].name, "width");
    assert_eq!(tree[0].children[1].kind, SymbolKind::Property);
    assert_eq!(tree[0].children[2].name, "height");
    assert_eq!(tree[0].children[2].kind, SymbolKind::Property);
}

#[test]
fn test_symbols_api_multiple_declarations_mixed_kinds() {
    let source = r#"
function alpha() {}
const beta = 42;
interface Gamma { x: number; }
type Delta = string[];
enum Epsilon { A, B }
class Zeta { method() {} }
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);

    assert_eq!(tree.len(), 6);
    assert_eq!(tree[0].name, "alpha");
    assert_eq!(tree[0].kind, SymbolKind::Function);
    assert_eq!(tree[1].name, "beta");
    assert_eq!(tree[1].kind, SymbolKind::Constant);
    assert_eq!(tree[2].name, "Gamma");
    assert_eq!(tree[2].kind, SymbolKind::Interface);
    assert_eq!(tree[3].name, "Delta");
    assert_eq!(tree[3].kind, SymbolKind::Struct);
    assert_eq!(tree[4].name, "Epsilon");
    assert_eq!(tree[4].kind, SymbolKind::Enum);
    assert_eq!(tree[5].name, "Zeta");
    assert_eq!(tree[5].kind, SymbolKind::Class);
}
