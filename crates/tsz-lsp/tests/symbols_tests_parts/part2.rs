#[test]
fn test_symbols_api_abstract_method_no_body() {
    let source =
        "abstract class Shape {\n  abstract area(): number;\n  abstract perimeter(): number;\n}";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Shape");
    assert!(
        tree[0].children.len() >= 2,
        "Should have abstract methods as children"
    );
}

#[test]
fn test_symbols_api_interface_with_call_signature() {
    let source = "interface Callable {\n  (x: number): string;\n}";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Callable");
}

#[test]
fn test_symbols_api_interface_with_construct_signature() {
    let source = "interface Constructor {\n  new (x: number): object;\n}";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Constructor");
}

#[test]
fn test_symbols_api_multiple_classes() {
    let source = "class A {}\nclass B {}\nclass C {}";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert_eq!(tree.len(), 3);
    assert_eq!(tree[0].name, "A");
    assert_eq!(tree[1].name, "B");
    assert_eq!(tree[2].name, "C");
}

#[test]
fn test_symbols_api_export_default_class() {
    let source = "export default class MyDefault {\n  method() {}\n}";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert!(!tree.is_empty());
}

#[test]
fn test_symbols_api_class_with_generic_method() {
    let source = "class Container {\n  get<T>(key: string): T { return {} as T; }\n}";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Container");
    assert!(!tree[0].children.is_empty());
}

#[test]
fn test_symbols_api_global_declare_var() {
    let source = "declare var process: any;";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "process");
}

#[test]
fn test_symbols_api_declare_const() {
    let source = "declare const VERSION: string;";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "VERSION");
}

#[test]
fn test_symbols_api_type_alias_with_keyof() {
    let source = "type Keys<T> = keyof T;";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Keys");
}

#[test]
fn test_symbols_api_multiple_interfaces() {
    let source =
        "interface A { x: number; }\ninterface B { y: string; }\ninterface C { z: boolean; }";
    let (parser, root) = parse_test_source(source);
    let symbols = DocumentSymbols::new(parser.get_arena(), source);
    let tree = symbols.get_symbol_tree(root);
    assert_eq!(tree.len(), 3);
    assert_eq!(tree[0].name, "A");
    assert_eq!(tree[1].name, "B");
    assert_eq!(tree[2].name, "C");
}
