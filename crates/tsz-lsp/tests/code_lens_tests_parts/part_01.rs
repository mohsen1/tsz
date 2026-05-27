#[test]
fn test_code_lens_single_line_function() {
    let source = "function id(x: number) { return x; }";
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_code_lens_function_returning_generic() {
    let source = "function wrap<T>(value: T): Array<T> {\n  return [value];\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Generic function should produce code lenses"
    );
    if let Some(lens) = lenses.iter().find(|l| l.range.start.line == 0) {
        assert!(lens.data.is_some(), "Lens should have data");
    }
}

#[test]
fn test_code_lens_interface_with_index_signature() {
    let source = "interface StringMap {\n  [key: string]: any;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    let iface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        !iface_lenses.is_empty(),
        "Interface with index signature should have lenses"
    );
}

#[test]
fn test_code_lens_class_with_multiple_constructors() {
    let source = "class Builder {\n  private items: string[] = [];\n  add(item: string) {\n    this.items.push(item);\n    return this;\n  }\n  build() {\n    return this.items;\n  }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        lenses.len() >= 2,
        "Class with multiple methods should produce multiple lenses, got {}",
        lenses.len()
    );
}

// =========================================================================
// Additional tests to reach 101+
// =========================================================================

#[test]
fn test_code_lens_class_with_private_static_method() {
    let source = "class Util {\n  private static helper() {}\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    // Should have lenses for the class and possibly the method
    assert!(
        !lenses.is_empty(),
        "Class with private static method should have lenses"
    );
}

#[test]
fn test_code_lens_function_expression_not_top_level() {
    let source = "const x = function myFunc() { return 1; };";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    // Named function expression may or may not get a lens; should not crash
    let _ = lenses;
}

#[test]
fn test_code_lens_multiple_enums() {
    let source = "enum A { X }\nenum B { Y }\nenum C { Z }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(
        lenses.len() >= 3,
        "Three enums should produce at least three lenses, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_class_with_method_overloads() {
    let source = "class Converter {\n  convert(x: string): number;\n  convert(x: number): string;\n  convert(x: any): any { return x; }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Class with method overloads should have lenses"
    );
}

#[test]
fn test_code_lens_interface_with_multiple_methods() {
    let source = "interface Service {\n  start(): void;\n  stop(): void;\n  restart(): void;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Interface with multiple methods should have lenses"
    );
}

#[test]
fn test_code_lens_type_alias_generic() {
    let source = "type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(!lenses.is_empty(), "Generic type alias should have a lens");
}

#[test]
fn test_code_lens_class_with_accessors() {
    let source = "class Config {\n  private _value = 0;\n  get value() { return this._value; }\n  set value(v: number) { this._value = v; }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Class with accessors should have lenses"
    );
}

#[test]
fn test_code_lens_exported_enum() {
    let source = "export enum Direction {\n  Up,\n  Down,\n  Left,\n  Right\n}";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_code_lens_class_with_async_method() {
    let source = "class Api {\n  async fetch() { return null; }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Class with async method should have lenses"
    );
}

#[test]
fn test_code_lens_type_alias_template_literal() {
    let source = "type EventName = `on${string}`;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Template literal type alias should have lenses"
    );
}

#[test]
fn test_code_lens_resolve_class_references() {
    let source = "class Foo {}\nlet a: Foo;\nlet b = new Foo();";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    if let Some(class_lens) = lenses.iter().find(|l| l.range.start.line == 0) {
        let resolved = provider.resolve_code_lens(root, class_lens);
        if let Some(resolved) = resolved {
            assert!(
                resolved.command.is_some(),
                "Resolved class lens should have command"
            );
        }
    }
}

#[test]
fn test_code_lens_resolve_enum_references() {
    let source = "enum Status { Active, Inactive }\nlet s: Status = Status.Active;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let lenses = provider.provide_code_lenses(root);
    if let Some(enum_lens) = lenses.iter().find(|l| l.range.start.line == 0) {
        let resolved = provider.resolve_code_lens(root, enum_lens);
        if let Some(resolved) = resolved {
            assert!(
                resolved.command.is_some(),
                "Resolved enum lens should have command"
            );
        }
    }
}
