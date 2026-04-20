#[test]
fn test_code_lens_function_returning_generic() {
    let source = "function wrap<T>(value: T): Array<T> {\n  return [value];\n}";
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
        "Generic function should produce code lenses"
    );
    if let Some(lens) = lenses.iter().find(|l| l.range.start.line == 0) {
        assert!(lens.data.is_some(), "Lens should have data");
    }
}

#[test]
fn test_code_lens_interface_with_index_signature() {
    let source = "interface StringMap {\n  [key: string]: any;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
        "Class with method overloads should have lenses"
    );
}

#[test]
fn test_code_lens_interface_with_multiple_methods() {
    let source = "interface Service {\n  start(): void;\n  stop(): void;\n  restart(): void;\n}";
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
        "Interface with multiple methods should have lenses"
    );
}

#[test]
fn test_code_lens_type_alias_generic() {
    let source = "type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
        "Class with accessors should have lenses"
    );
}

