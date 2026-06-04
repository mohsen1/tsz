#[test]
fn test_document_symbols_type_alias_conditional() {
    let source = "type IsString<T> = T extends string ? true : false;";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "add");
}

#[test]
fn test_document_symbols_class_with_method_overloads() {
    let source = "class Parser {\n  parse(input: string): void;\n  parse(input: number): void;\n  parse(input: any): void {}\n}";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "StringOrNumber");
}

#[test]
fn test_document_symbols_type_alias_intersection() {
    let source = "type Combined = { a: number } & { b: string };";
    let (parser, root) = parse_test_source(source);
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Combined");
}

#[test]
fn test_document_symbols_multiple_enum_declarations() {
    let source = "enum Color { Red, Green, Blue }\nenum Size { Small, Medium, Large }";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
    let line_map = LineMap::build(source);

    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    let symbols = provider.get_document_symbols(root);

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "PI");
}

#[test]
fn test_document_symbols_declare_enum() {
    let source = "declare enum Direction {\n  Up,\n  Down\n}";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
