#[test]
fn test_code_lens_mixed_declarations() {
    let source = "function a() {}\nclass B {}\ninterface C {}\nenum D { X }\ntype E = number;\nfunction f() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for all declaration types
    assert!(
        lenses.len() >= 6,
        "Should have at least 6 lenses for mixed declarations, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_interface_extending_interface() {
    let source =
        "interface Base {\n  id: number;\n}\ninterface Child extends Base {\n  name: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Both interfaces should have lenses
    let has_base = lenses.iter().any(|l| l.range.start.line == 0);
    let has_child = lenses.iter().any(|l| l.range.start.line == 3);
    assert!(has_base, "Base interface should have a code lens");
    assert!(has_child, "Child interface should have a code lens");
}

#[test]
fn test_code_lens_file_path_in_data() {
    let source = "function test() {}";
    let file_path = "src/components/widget.ts";
    let mut parser = ParserState::new(file_path.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, file_path.to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    for lens in &lenses {
        if let Some(data) = &lens.data {
            assert_eq!(
                data.file_path, file_path,
                "Lens data should contain the correct file path"
            );
        }
    }
}

#[test]
fn test_code_lens_async_function() {
    let source = "async function fetchData() {\n  return await Promise.resolve(1);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let func_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        func_lens.is_some(),
        "Async function should have a code lens"
    );
}

#[test]
fn test_code_lens_generator_function() {
    let source = "function* gen() {\n  yield 1;\n  yield 2;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let func_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        func_lens.is_some(),
        "Generator function should have a code lens"
    );
}

#[test]
fn test_code_lens_class_with_private_method() {
    let source = "class Secret {\n  private hidden() {}\n  public visible() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Class should still get a lens regardless of member visibility
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with private methods should have a code lens"
    );
}

#[test]
fn test_code_lens_interface_with_readonly_properties() {
    let source = "interface Immutable {\n  readonly x: number;\n  readonly y: string;\n}";
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
        "Interface with readonly properties should have refs and impls lenses"
    );
}

#[test]
fn test_code_lens_enum_with_computed_values() {
    let source =
        "enum FileAccess {\n  Read = 1 << 0,\n  Write = 1 << 1,\n  ReadWrite = Read | Write\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let enum_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        enum_lens.is_some(),
        "Enum with computed values should have a code lens"
    );
}

#[test]
fn test_code_lens_type_alias_union_of_interfaces() {
    let source = "interface A {}\ninterface B {}\ntype AB = A | B;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for both interfaces and the type alias
    assert!(
        lenses.len() >= 3,
        "Should have lenses for 2 interfaces + 1 type alias, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_class_with_index_signature() {
    let source = "class DynamicObj {\n  [key: string]: any;\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with index signature should have a code lens"
    );
}

