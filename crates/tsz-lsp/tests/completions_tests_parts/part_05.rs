#[test]
fn test_completions_in_nested_function() {
    let source = "const a = 1;\nfunction outer() {\n  const b = 2;\n  function inner() {\n    const c = 3;\n    \n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    // Inside inner function
    let items = completions.get_completions(root, Position::new(5, 4));
    assert!(
        items.is_some(),
        "Should have completions in nested function"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"a"), "Should suggest top-level 'a'");
        assert!(names.contains(&"b"), "Should suggest outer 'b'");
        assert!(names.contains(&"c"), "Should suggest inner 'c'");
    }
}

#[test]
fn test_completions_in_if_block() {
    let source = "const x = 1;\nif (true) {\n  const y = 2;\n  \n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(3, 2));
    assert!(items.is_some(), "Should have completions in if block");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"x"), "Should see outer 'x'");
        assert!(names.contains(&"y"), "Should see block-scoped 'y'");
    }
}

#[test]
fn test_completions_enum_members() {
    let source = "enum Color { Red, Green, Blue }\nColor.";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let items = completions.get_completions(root, Position::new(1, 6));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"Red"), "Should suggest 'Red'");
        assert!(names.contains(&"Green"), "Should suggest 'Green'");
        assert!(names.contains(&"Blue"), "Should suggest 'Blue'");
    }
}

#[test]
fn test_completions_interface_as_type() {
    let source = "interface Foo { bar: number; }\nlet x: ";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(1, 7));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"Foo"),
            "Should suggest interface 'Foo' as type"
        );
    }
}

#[test]
fn test_completions_multiple_files_via_project() {
    let mut project = crate::Project::new();
    project.set_file("a.ts".to_string(), "export const shared = 42;".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { shared } from './a';\n".to_string(),
    );

    let completions = project.get_completions("b.ts", Position::new(1, 0));
    assert!(
        completions.is_some(),
        "Should get completions in multi-file project"
    );
    if let Some(items) = completions {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"shared"),
            "Should suggest imported 'shared'"
        );
    }
}

#[test]
fn test_completions_destructuring_context() {
    let source = "const obj = { foo: 1, bar: 2 };\nconst { } = obj;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    // Inside destructuring braces
    let items = completions.get_completions(root, Position::new(1, 8));
    // Should not panic; may or may not have specific property completions
    let _ = items;
}

#[test]
fn test_completions_switch_case() {
    let source = "const val = 1;\nswitch (val) {\n  case 1:\n    \n    break;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(3, 4));
    assert!(items.is_some(), "Should have completions in switch case");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"val"),
            "Should suggest 'val' in switch case"
        );
    }
}

#[test]
fn test_completions_try_catch() {
    let source = "const x = 1;\ntry {\n  const y = 2;\n  \n} catch (e) {\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(3, 2));
    assert!(items.is_some(), "Should have completions in try block");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"x"), "Should suggest outer 'x'");
        assert!(names.contains(&"y"), "Should suggest try-scoped 'y'");
    }
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_completions_no_completions_in_line_comment() {
    // Cursor inside a line comment should yield empty completions
    let source = "const x = 1;\n// some comment ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 16));
    // Inside a comment, completions should be suppressed (empty list)
    if let Some(ref items) = items {
        assert!(
            items.is_empty(),
            "Should not have completions inside a line comment"
        );
    }
}

#[test]
fn test_completions_no_completions_in_block_comment() {
    // Cursor inside a block comment should yield empty completions
    let source = "const x = 1;\n/* block comment */";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the block comment
    let items = completions.get_completions(root, Position::new(1, 10));
    if let Some(ref items) = items {
        assert!(
            items.is_empty(),
            "Should not have completions inside a block comment"
        );
    }
}

