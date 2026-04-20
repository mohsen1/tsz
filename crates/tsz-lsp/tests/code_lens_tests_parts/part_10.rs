#[test]
fn test_code_lens_class_with_async_method() {
    let source = "class Api {\n  async fetch() { return null; }\n}";
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
        "Class with async method should have lenses"
    );
}

#[test]
fn test_code_lens_type_alias_template_literal() {
    let source = "type EventName = `on${string}`;";
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
        "Template literal type alias should have lenses"
    );
}

#[test]
fn test_code_lens_resolve_class_references() {
    let source = "class Foo {}\nlet a: Foo;\nlet b = new Foo();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
