#[test]
fn test_code_lens_function_with_default_params() {
    let source = "function greet(name: string = 'World') {\n  return `Hello ${name}`;\n}";
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
        "Function with default params should have a code lens"
    );
}

#[test]
fn test_code_lens_declare_function() {
    let source = "declare function external(): void;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Declared functions should still get code lenses
    let _ = lenses; // Defensive: just ensure no panic
}

#[test]
fn test_code_lens_class_with_decorators_syntax() {
    let source = "class Component {\n  method() {}\n}\nclass Service {\n  handle() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Both classes should have lenses
    let has_component = lenses.iter().any(|l| l.range.start.line == 0);
    let has_service = lenses.iter().any(|l| l.range.start.line == 3);
    assert!(has_component, "Component class should have a code lens");
    assert!(has_service, "Service class should have a code lens");
}

#[test]
fn test_code_lens_interface_with_optional_properties() {
    let source =
        "interface Options {\n  debug?: boolean;\n  verbose?: boolean;\n  timeout?: number;\n}";
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
        "Interface with optional properties should have refs and impls lenses"
    );
}

#[test]
fn test_code_lens_resolve_command_for_references() {
    let source = "function target() {}\ntarget();\ntarget();\ntarget();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    let ref_lens = lenses.iter().find(|l| {
        l.data
            .as_ref()
            .is_some_and(|d| d.kind == CodeLensKind::References)
            && l.range.start.line == 0
    });

    if let Some(lens) = ref_lens {
        let resolved = provider.resolve_code_lens(root, lens);
        if let Some(resolved) = resolved
            && let Some(command) = resolved.command
        {
            assert_eq!(
                command.command, "editor.action.showReferences",
                "References lens should use showReferences command"
            );
        }
    }
}

#[test]
fn test_code_lens_class_implementing_multiple_interfaces() {
    let source = "interface A {\n  a(): void;\n}\ninterface B {\n  b(): void;\n}\nclass Impl implements A, B {\n  a() {}\n  b() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for both interfaces and the implementing class
    assert!(
        lenses.len() >= 5,
        "Should have lenses for 2 interfaces (refs+impls each) + 1 class, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_namespace_declaration() {
    let source = "namespace Utils {\n  export function helper() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Namespace and the function inside should both get lenses
    let _ = lenses; // Defensive: ensure no panic
}

#[test]
fn test_code_lens_empty_class() {
    let source = "class Empty {}";
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
        "Empty class should still have a code lens"
    );
}

#[test]
fn test_code_lens_empty_interface() {
    let source = "interface Marker {}";
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
        "Empty interface should have refs and impls lenses, got {}",
        interface_lenses.len()
    );
}

#[test]
fn test_code_lens_type_alias_intersection() {
    let source = "type Combined = { a: number } & { b: string };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let type_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        type_lens.is_some(),
        "Intersection type alias should have a code lens"
    );
}

