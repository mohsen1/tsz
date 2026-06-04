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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_completions_no_completions_in_string_literal() {
    // Cursor inside a string literal should yield empty completions (non-module-specifier)
    let source = "const x = \"hello world\";\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the string literal
    let items = completions.get_completions(root, Position::new(0, 15));
    if let Some(ref items) = items {
        assert!(
            items.is_empty(),
            "Should not have completions inside a string literal"
        );
    }
}

#[test]
fn test_completions_no_completions_after_double_dot() {
    // After ".." (not "..." spread), no completions should be offered
    let source = "const x = [1, 2];\nx..";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 3));
    assert!(
        items.is_none(),
        "Should not have completions after double dot '..'"
    );
}

#[test]
fn test_completions_at_start_of_file_with_content() {
    // Cursor at position (0,0) when the file has content
    let source = "const x = 1;\nconst y = 2;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 0));
    // At position 0,0, completions should include keywords at minimum
    assert!(items.is_some(), "Should have completions at start of file");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Keywords should always be available
        assert!(
            names.contains(&"const"),
            "Should suggest keyword 'const' at start of file"
        );
    }
}

#[test]
fn test_completions_function_snippet_insert_text_with_helper() {
    // Functions should have insert text with snippet tab-stop
    let source = "function myFunc() {}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let func_item = items.iter().find(|i| i.label == "myFunc");
        assert!(func_item.is_some(), "Should find 'myFunc' completion");
        let func_item = func_item.unwrap();
        assert_eq!(
            func_item.kind,
            CompletionItemKind::Function,
            "myFunc should be Function kind"
        );
        assert!(func_item.is_snippet, "Function should be a snippet");
        assert_eq!(
            func_item.insert_text.as_deref(),
            Some("myFunc($1)"),
            "Function insert text should include tab-stop"
        );
    }
}

#[test]
fn test_completions_arguments_available_inside_function() {
    // Inside a function body, "arguments" should appear as a completion
    let source = "function foo() {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 2));
    assert!(items.is_some(), "Should have completions in function body");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"arguments"),
            "Should suggest 'arguments' inside function"
        );
    }
}

#[test]
fn test_completions_arguments_not_at_top_level() {
    // At the top level (outside any function), "arguments" should NOT appear
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions at top level");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            !names.contains(&"arguments"),
            "Should NOT suggest 'arguments' at top level"
        );
    }
}

#[test]
fn test_completions_keywords_at_top_level() {
    // Keywords like if, for, while, return should appear in completions
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"if"), "Should suggest keyword 'if'");
        assert!(names.contains(&"for"), "Should suggest keyword 'for'");
        assert!(names.contains(&"while"), "Should suggest keyword 'while'");
        assert!(
            names.contains(&"function"),
            "Should suggest keyword 'function'"
        );
        assert!(names.contains(&"class"), "Should suggest keyword 'class'");

        // Keywords should have Keyword kind and appropriate sort text
        let if_item = items.iter().find(|i| i.label == "if").unwrap();
        assert_eq!(if_item.kind, CompletionItemKind::Keyword);
        assert_eq!(
            if_item.sort_text.as_deref(),
            Some(sort_priority::KEYWORD),
            "Keywords should have KEYWORD sort priority"
        );
    }
}

#[test]
fn test_completions_global_this_always_present() {
    // globalThis should always appear in global completions
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"globalThis"), "Should suggest 'globalThis'");
        assert!(names.contains(&"undefined"), "Should suggest 'undefined'");
    }
}

#[test]
fn test_completions_const_vs_let_kind() {
    // const and let declarations should have distinct CompletionItemKind
    let source = "const myConst = 1;\nlet myLet = 2;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let const_item = items.iter().find(|i| i.label == "myConst");
        let let_item = items.iter().find(|i| i.label == "myLet");
        assert!(const_item.is_some(), "Should find 'myConst'");
        assert!(let_item.is_some(), "Should find 'myLet'");
        assert_eq!(
            const_item.unwrap().kind,
            CompletionItemKind::Const,
            "const declaration should have Const kind"
        );
        assert_eq!(
            let_item.unwrap().kind,
            CompletionItemKind::Let,
            "let declaration should have Let kind"
        );
    }
}
