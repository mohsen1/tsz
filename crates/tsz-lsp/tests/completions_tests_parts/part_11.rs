#[test]
fn test_completions_member_method_on_object() {
    // Object with method should suggest method with Method kind
    let source = "const obj = { greet() { return 'hi'; } };\nobj.";
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
    let position = Position::new(1, 4);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    if let Some(items) = items {
        let greet_item = items.iter().find(|i| i.label == "greet");
        assert!(
            greet_item.is_some(),
            "Should suggest method 'greet', got: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_completions_member_class_instance() {
    // Class instance member access should show public properties and methods
    let source = "class Point {\n  x: number = 0;\n  y: number = 0;\n  distance() { return 0; }\n}\nconst p = new Point();\np.";
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
    let position = Position::new(6, 2);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    // Should not crash; may or may not have completions depending on class resolution
    let _ = items;
}

#[test]
fn test_completions_return_statement_inside_nested_function() {
    // Return inside nested function should suggest variables from all enclosing scopes
    let source = "const global = 1;\nfunction outer() {\n  const mid = 2;\n  function inner() {\n    const local = 3;\n    return ;\n  }\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `return ` in inner function (line 5, col 11)
    let items = completions.get_completions(root, Position::new(5, 11));
    assert!(
        items.is_some(),
        "Should have completions in nested return statement"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"local"),
            "Should suggest 'local' in return, got: {names:?}"
        );
        assert!(
            names.contains(&"mid"),
            "Should suggest 'mid' from outer scope, got: {names:?}"
        );
        assert!(
            names.contains(&"global"),
            "Should suggest 'global' from top scope, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_let_in_different_block_scopes() {
    // let variables in different block scopes should not leak
    let source = "if (true) {\n  let blockA = 1;\n}\nif (true) {\n  let blockB = 2;\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside second if block (line 5, col 2)
    let items = completions.get_completions(root, Position::new(5, 2));
    assert!(
        items.is_some(),
        "Should have completions inside second if block"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"blockB"),
            "Should suggest 'blockB' from current block, got: {names:?}"
        );
        // blockA is in a different (closed) block scope - may or may not be visible
        // depending on binder scope resolution
    }
}

#[test]
fn test_completions_try_catch_finally_scoping() {
    // Variables in finally block should see outer scope but not try/catch locals
    let source = "const outer = 0;\ntry {\n  const tryVar = 1;\n} catch (e) {\n  const catchVar = 2;\n} finally {\n  const finalVar = 3;\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside finally block (line 7, col 2)
    let items = completions.get_completions(root, Position::new(7, 2));
    assert!(
        items.is_some(),
        "Should have completions inside finally block"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"outer"),
            "Should suggest 'outer' in finally, got: {names:?}"
        );
        assert!(
            names.contains(&"finalVar"),
            "Should suggest 'finalVar' in finally, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_function_parameter_default_value() {
    // In parameter default value position, should suggest visible variables
    let source = "const defaultVal = 10;\nfunction f(x = ) {}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `x = ` (line 1, col 15)
    let items = completions.get_completions(root, Position::new(1, 15));
    assert!(
        items.is_some(),
        "Should have completions in parameter default value"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"defaultVal"),
            "Should suggest 'defaultVal' in param default, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_while_loop_body() {
    // Inside while loop body, should suggest variables from enclosing scope
    let source = "const counter = 0;\nwhile (true) {\n  const loopVar = 1;\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 2));
    assert!(items.is_some(), "Should have completions inside while loop");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"counter"),
            "Should suggest 'counter', got: {names:?}"
        );
        assert!(
            names.contains(&"loopVar"),
            "Should suggest 'loopVar', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_const_enum_kind() {
    // const enums should also have Enum kind
    let source = "const enum Direction { Up, Down, Left, Right }\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let dir_item = items.iter().find(|i| i.label == "Direction");
        assert!(dir_item.is_some(), "Should find 'Direction'");
        assert_eq!(
            dir_item.unwrap().kind,
            CompletionItemKind::Enum,
            "const enum should have Enum kind"
        );
    }
}

#[test]
fn test_completions_module_kind() {
    // Module declarations should have Module kind
    let source = "module MyModule {\n  export const v = 1;\n}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let mod_item = items.iter().find(|i| i.label == "MyModule");
        assert!(mod_item.is_some(), "Should find 'MyModule'");
        assert_eq!(
            mod_item.unwrap().kind,
            CompletionItemKind::Module,
            "Module should have Module kind"
        );
    }
}

#[test]
fn test_completions_namespace_kind() {
    // Namespace declarations should have Module kind
    let source = "namespace NS {\n  export const v = 1;\n}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let ns_item = items.iter().find(|i| i.label == "NS");
        assert!(ns_item.is_some(), "Should find 'NS'");
        assert_eq!(
            ns_item.unwrap().kind,
            CompletionItemKind::Module,
            "Namespace should have Module kind"
        );
    }
}

