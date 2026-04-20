#[test]
fn test_code_lens_deeply_nested_class() {
    let source = "namespace Outer {\n  namespace Inner {\n    class Deep {\n      method() {}\n    }\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Deeply nested class should produce code lenses"
    );
}

#[test]
fn test_code_lens_class_with_readonly_property() {
    let source = "class Config {\n  readonly host: string = 'localhost';\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Class with readonly property should have lenses"
    );
}

#[test]
fn test_code_lens_type_alias_conditional() {
    let source = "type IsString<T> = T extends string ? true : false;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Conditional type alias should produce code lenses"
    );
}

#[test]
fn test_code_lens_type_alias_mapped() {
    let source = "type Partial<T> = {\n  [K in keyof T]?: T[K];\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Mapped type alias should produce code lenses"
    );
}

#[test]
fn test_code_lens_function_with_optional_params() {
    let source = "function greet(name?: string, greeting?: string) {\n  return `Hello`;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Function with optional params should have lenses"
    );
    let func_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(func_lens.is_some(), "Should have lens at line 0");
}

#[test]
fn test_code_lens_interface_with_generic_params() {
    let source = "interface Repository<T, K extends string> {\n  get(id: K): T;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Generic interface should have refs and impls lenses, got {}",
        interface_lenses.len()
    );
}

#[test]
fn test_code_lens_enum_string_members() {
    let source = "enum Status {\n  Active = 'active',\n  Inactive = 'inactive'\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Enum with string members should produce code lenses"
    );
}

#[test]
fn test_code_lens_multiple_namespaces() {
    let source =
        "namespace A {\n  export function fa() {}\n}\nnamespace B {\n  export function fb() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        lenses.len() >= 2,
        "Multiple namespaces should produce multiple lenses, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_class_with_abstract_property() {
    let source =
        "abstract class Shape {\n  abstract area: number;\n  abstract perimeter(): number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Abstract class with abstract members should produce lenses"
    );
}

#[test]
fn test_code_lens_single_line_function() {
    let source = "function id(x: number) { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Single-line function should still produce code lenses"
    );
}

