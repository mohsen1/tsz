#[test]
fn test_code_lens_interface_method() {
    let source = "interface Greeter {\n  greet(name: string): string;\n  wave(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Interface should get references + implementations lenses
    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Interface should have references and implementations lenses, got {}",
        interface_lenses.len()
    );

    let has_refs = interface_lenses
        .iter()
        .any(|l| l.data.as_ref().unwrap().kind == CodeLensKind::References);
    let has_impls = interface_lenses
        .iter()
        .any(|l| l.data.as_ref().unwrap().kind == CodeLensKind::Implementations);
    assert!(has_refs, "Interface should have references lens");
    assert!(has_impls, "Interface should have implementations lens");
}

#[test]
fn test_code_lens_class_with_constructor_and_methods() {
    let source = "class Animal {\n  constructor(public name: string) {}\n  speak() {\n    return this.name;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lens for class declaration at minimum
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Should have lens for class declaration"
    );

    // Should have lenses for methods too
    assert!(
        lenses.len() >= 2,
        "Should have lenses for class and at least one method, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_multiple_classes() {
    let source = "class A {\n  foo() {}\n}\nclass B {\n  bar() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for both classes and their methods
    assert!(
        lenses.len() >= 4,
        "Should have at least 4 lenses (2 classes + 2 methods), got {}",
        lenses.len()
    );

    let has_class_a = lenses.iter().any(|l| l.range.start.line == 0);
    let has_class_b = lenses.iter().any(|l| l.range.start.line == 3);
    assert!(has_class_a, "Should have lens for class A at line 0");
    assert!(has_class_b, "Should have lens for class B at line 3");
}

#[test]
fn test_code_lens_exported_function() {
    let source = "export function greet() {}\nexport function farewell() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Exported functions should still get lenses
    assert!(
        lenses.len() >= 2,
        "Should have lenses for exported functions, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_resolve_zero_references() {
    let source = "function unused() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(!lenses.is_empty(), "Should have a lens for the function");

    let func_lens = &lenses[0];
    let resolved = provider.resolve_code_lens(root, func_lens);

    assert!(resolved.is_some(), "Should resolve lens");
    let resolved = resolved.unwrap();
    let command = resolved.command.unwrap();
    assert!(
        command.title.contains("0 references"),
        "Unused function should show 0 references, got: {}",
        command.title
    );
}

#[test]
fn test_code_lens_all_have_data() {
    let source = "function a() {}\nclass B {}\ninterface C {}\nenum D { X }\ntype E = string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // All unresolved lenses should have data with file_path set
    for lens in &lenses {
        assert!(lens.data.is_some(), "All lenses should have data");
        let data = lens.data.as_ref().unwrap();
        assert_eq!(
            data.file_path, "test.ts",
            "Lens data should have correct file path"
        );
    }
}

#[test]
fn test_code_lens_abstract_class() {
    let source = "abstract class Base {\n  abstract foo(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Abstract class should get a code lens
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Abstract class should have a code lens"
    );
}

#[test]
fn test_code_lens_const_enum() {
    let source = "const enum Direction {\n  Up,\n  Down,\n  Left,\n  Right\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // const enum should get a code lens
    let enum_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(enum_lens.is_some(), "Const enum should have a code lens");
}

#[test]
fn test_code_lens_interface_only_has_implementations_kind() {
    let source = "interface Serializable {\n  serialize(): string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Interface should have an Implementations kind lens
    let impl_lens = lenses.iter().find(|l| {
        l.data
            .as_ref()
            .is_some_and(|d| d.kind == CodeLensKind::Implementations)
    });
    assert!(
        impl_lens.is_some(),
        "Interface should have an Implementations lens"
    );
}

#[test]
fn test_code_lens_class_no_implementations_kind() {
    // Regular classes should not get an Implementations lens (only interfaces do)
    let source = "class Concrete {\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let impl_lens = lenses.iter().find(|l| {
        l.data
            .as_ref()
            .is_some_and(|d| d.kind == CodeLensKind::Implementations)
    });
    assert!(
        impl_lens.is_none(),
        "Regular class should NOT have an Implementations lens"
    );
}

