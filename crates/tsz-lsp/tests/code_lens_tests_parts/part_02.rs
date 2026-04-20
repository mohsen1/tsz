#[test]
fn test_code_lens_resolve_interface_implementations() {
    let source = "interface Runnable {\n  run(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Find the implementations lens and resolve it
    let impl_lens = lenses.iter().find(|l| {
        l.data
            .as_ref()
            .is_some_and(|d| d.kind == CodeLensKind::Implementations)
    });

    if let Some(lens) = impl_lens {
        let resolved = provider.resolve_code_lens(root, lens);
        assert!(resolved.is_some(), "Implementations lens should resolve");
        let resolved = resolved.unwrap();
        assert!(
            resolved.command.is_some(),
            "Resolved lens should have command"
        );
        let cmd = resolved.command.unwrap();
        assert_eq!(cmd.command, "editor.action.goToImplementation");
    }
}

#[test]
fn test_code_lens_resolve_with_no_data() {
    let source = "function foo() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Create a lens with no data - resolution should return None
    let range = tsz_common::position::Range::new(Position::new(0, 0), Position::new(0, 1));
    let lens = CodeLens {
        range,
        command: None,
        data: None,
    };

    let resolved = provider.resolve_code_lens(root, &lens);
    assert!(resolved.is_none(), "Lens without data should not resolve");
}

#[test]
fn test_code_lens_multiple_interfaces() {
    let source = "interface A {\n  x: number;\n}\ninterface B {\n  y: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Each interface should get both references and implementations lenses
    let interface_a_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    let interface_b_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 3).collect();

    assert!(
        interface_a_lenses.len() >= 2,
        "Interface A should have at least 2 lenses (refs + impls), got {}",
        interface_a_lenses.len()
    );
    assert!(
        interface_b_lenses.len() >= 2,
        "Interface B should have at least 2 lenses (refs + impls), got {}",
        interface_b_lenses.len()
    );
}

#[test]
fn test_code_lens_only_comments() {
    let source = "// This is a comment\n/* Block comment */";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    assert!(
        lenses.is_empty(),
        "File with only comments should have no code lenses"
    );
}

#[test]
fn test_code_lens_generic_function() {
    let source = "function identity<T>(arg: T): T {\n  return arg;\n}";
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
        "Generic function should have a code lens"
    );
}

#[test]
fn test_code_lens_resolve_single_reference() {
    let source = "function greet() {}\ngreet();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    let func_lens = lenses
        .iter()
        .find(|l| l.range.start.line == 0)
        .expect("Should have function lens");

    let resolved = provider.resolve_code_lens(root, func_lens);
    if let Some(resolved) = resolved
        && let Some(command) = resolved.command
    {
        // Reference count depends on binder implementation
        assert!(
            command.title.contains("reference"),
            "Should contain 'reference' in title, got: {}",
            command.title
        );
    }
}

#[test]
fn test_code_lens_class_with_static_method() {
    let source = "class Utils {\n  static parse() {}\n  static format() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lens for the class itself at minimum
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with static methods should have a lens"
    );
}

#[test]
fn test_code_lens_arrow_function_variable() {
    let source = "const greet = () => 'hello';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Arrow functions assigned to variables typically don't get code lenses
    // Defensive: just verify no panic
    let _ = lenses;
}

#[test]
fn test_code_lens_class_getter_setter() {
    let source = "class Obj {\n  get value() { return 1; }\n  set value(v: number) {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have at least one lens for the class
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with getter/setter should have a code lens"
    );
}

#[test]
fn test_code_lens_function_overloads() {
    let source = "function foo(x: string): string;\nfunction foo(x: number): number;\nfunction foo(x: any): any { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for the overloaded function declarations
    assert!(!lenses.is_empty(), "Function overloads should have lenses");
}

