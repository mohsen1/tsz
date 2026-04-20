#[test]
fn test_code_lens_exported_class() {
    let source = "export class Widget {\n  render() {}\n}";
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
        "Exported class should have a code lens"
    );
}

#[test]
fn test_code_lens_exported_interface() {
    let source = "export interface Config {\n  debug: boolean;\n  port: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Exported interface should still get lenses
    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Exported interface should have references and implementations lenses, got {}",
        interface_lenses.len()
    );
}

#[test]
fn test_code_lens_enum_with_values() {
    let source =
        "enum Status {\n  Active = 'active',\n  Inactive = 'inactive',\n  Pending = 'pending'\n}";
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
        "Enum with string values should have a code lens"
    );
}

#[test]
fn test_code_lens_class_with_properties() {
    let source = "class Config {\n  public host: string = 'localhost';\n  private port: number = 3000;\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lens for class at minimum
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with properties should have a code lens"
    );
}

#[test]
fn test_code_lens_generic_class() {
    let source = "class Container<T> {\n  value: T;\n  get(): T { return this.value; }\n}";
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
        "Generic class should have a code lens"
    );
}

#[test]
fn test_code_lens_generic_interface() {
    let source = "interface Repository<T> {\n  find(id: string): T;\n  save(item: T): void;\n}";
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
        "Generic interface should have references and implementations lenses"
    );
}

#[test]
fn test_code_lens_class_extending_class() {
    let source = "class Animal {\n  move() {}\n}\nclass Dog extends Animal {\n  bark() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Both classes should have lenses
    let has_animal = lenses.iter().any(|l| l.range.start.line == 0);
    let has_dog = lenses.iter().any(|l| l.range.start.line == 3);
    assert!(has_animal, "Base class should have a code lens");
    assert!(has_dog, "Derived class should have a code lens");
}

#[test]
fn test_code_lens_class_implementing_interface() {
    let source = "interface Printable {\n  print(): void;\n}\nclass Report implements Printable {\n  print() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Interface should have implementations lens
    let impl_lens = lenses.iter().find(|l| {
        l.range.start.line == 0
            && l.data
                .as_ref()
                .is_some_and(|d| d.kind == CodeLensKind::Implementations)
    });
    assert!(
        impl_lens.is_some(),
        "Interface with implementing class should have implementations lens"
    );
}

#[test]
fn test_code_lens_resolve_many_references() {
    let source = "function used() {}\nused();\nused();\nused();\nused();\nused();";
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
        assert!(
            command.title.contains("reference"),
            "Should contain 'reference' in title, got: {}",
            command.title
        );
    }
}

#[test]
fn test_code_lens_abstract_method() {
    let source =
        "abstract class Shape {\n  abstract area(): number;\n  abstract perimeter(): number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Abstract class should get a lens
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Abstract class with abstract methods should have lens"
    );
}

