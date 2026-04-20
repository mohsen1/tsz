#[test]
fn test_code_lens_exported_enum() {
    let source = "export enum Direction {\n  Up,\n  Down,\n  Left,\n  Right\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(!lenses.is_empty(), "Exported enum should have lenses");
}

#[test]
fn test_code_lens_exported_type_alias() {
    let source = "export type ID = string | number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(!lenses.is_empty(), "Exported type alias should have lenses");
}

#[test]
fn test_code_lens_class_with_heritage_and_methods() {
    let source = "interface Base { run(): void; }\nclass Impl implements Base {\n  run() {}\n  extra() {}\n}";
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
        "Interface + class should produce multiple lenses"
    );
}

#[test]
fn test_code_lens_declare_class() {
    let source = "declare class External {\n  method(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(!lenses.is_empty(), "Declare class should have lenses");
}

#[test]
fn test_code_lens_declare_interface() {
    let source = "declare interface ExternalApi {\n  fetch(url: string): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(!lenses.is_empty(), "Declare interface should have lenses");
}

#[test]
fn test_code_lens_function_with_destructured_params() {
    let source = "function process({ name, age }: { name: string; age: number }) {}";
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
        "Function with destructured params should have lenses"
    );
}

#[test]
fn test_code_lens_class_with_computed_property() {
    let source = "class Store {\n  [Symbol.iterator]() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    // Should at least have a lens for the class
    assert!(
        !lenses.is_empty(),
        "Class with computed property should have lenses"
    );
}

#[test]
fn test_code_lens_interface_with_construct_signature() {
    let source = "interface Constructor {\n  new (name: string): object;\n}";
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
        "Interface with construct signature should have lenses"
    );
}

#[test]
fn test_code_lens_multiple_functions_and_classes() {
    let source = "function a() {}\nfunction b() {}\nclass C {}\nclass D {}\ninterface E {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(
        lenses.len() >= 5,
        "Five declarations should produce at least five lenses, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_enum_with_string_and_numeric_members() {
    let source = "enum Mixed {\n  A = 0,\n  B = \"bee\",\n  C = 2\n}";
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
        "Enum with mixed members should have lenses"
    );
}

