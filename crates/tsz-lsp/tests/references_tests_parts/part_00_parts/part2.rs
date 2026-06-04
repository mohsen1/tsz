#[test]
fn test_find_references_for_of_loop_variable() {
    // for-of loop variable
    let source = "const arr = [1, 2, 3];\nfor (const item of arr) {\n  console.log(item);\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'item' usage inside the loop body (line 2, col 14)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(2, 14));

    assert!(
        refs.is_some(),
        "Should find references for for-of loop variable item"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find loop variable declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_detailed_refs_postfix_increment_is_write() {
    // x++ should be detected as a write access
    let source = "let counter = 0;\ncounter++;\nconsole.log(counter);";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // The postfix increment (line 1) should be a write access
    let inc_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    if let Some(inc_ref) = inc_ref {
        assert!(
            inc_ref.is_write_access,
            "Postfix increment should be a write access"
        );
        assert!(
            !inc_ref.is_definition,
            "Postfix increment should not be a definition"
        );
    }
}

#[test]
fn test_find_references_array_destructured_variable() {
    // Array destructuring
    let source = "const [first, second] = [1, 2];\nconsole.log(first);\nlet x = second + first;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'first' usage on line 1, col 12
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 12));

    assert!(
        refs.is_some(),
        "Should find references for array-destructured variable first"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find array binding + 2 usages of first, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_class_name_across_usages() {
    let source = "class Widget {}\nconst w = new Widget();\nlet x: Widget;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Widget' declaration (line 0, col 6)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));

    assert!(refs.is_some(), "Should find references for class name");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find class declaration + new expression + type annotation, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_interface_name_in_type_position() {
    let source = "interface Config { key: string; }\nfunction init(c: Config) {}\nconst cfg: Config = { key: 'a' };";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Config' declaration (line 0, col 10)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));

    assert!(refs.is_some(), "Should find references for interface name");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find interface decl + 2 type usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_namespace_name() {
    let source = "namespace Utils {\n  export function helper() {}\n}\nUtils.helper();";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Utils' usage (line 3, col 0)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(3, 0));

    assert!(refs.is_some(), "Should find references for namespace name");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find namespace declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_empty_file_returns_none() {
    let source = "";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 0));

    assert!(refs.is_none(), "Empty file should return None");
}

#[test]
fn test_find_references_for_loop_counter() {
    let source = "for (let i = 0; i < 5; i++) {\n  console.log(i);\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'i' declaration (line 0, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 9));

    assert!(
        refs.is_some(),
        "Should find references for for-loop counter"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find declaration + condition + increment + body usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_arrow_function_param() {
    let source = "const double = (n: number) => n * 2;\ndouble(3);";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'n' parameter (col 16)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 16));

    assert!(
        refs.is_some(),
        "Should find references for arrow function param"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find param declaration + usage in body, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_nested_function_scoping() {
    let source = "function outer() {\n  const x = 1;\n  function inner() {\n    const x = 2;\n    x;\n  }\n  x;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'x' in outer scope (line 1, col 8)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 8));

    assert!(refs.is_some(), "Should find references for outer x");
    let refs = refs.unwrap();
    // Outer x should have declaration + usage on line 6, but NOT include inner x
    assert!(
        refs.len() >= 2,
        "Should find outer x declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_type_alias_in_multiple_annotations() {
    let source = "type ID = string;\nlet a: ID;\nlet b: ID;\nfunction process(id: ID) {}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'ID' declaration (line 0, col 5)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for type alias");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 4,
        "Should find type alias decl + 3 type usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_const_enum_name() {
    let source = "const enum Fruit { Apple, Banana }\nlet f: Fruit = Fruit.Apple;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Fruit' declaration (line 0, col 11)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 11));

    assert!(refs.is_some(), "Should find references for const enum");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find const enum declaration + usages, got {}",
        refs.len()
    );
}
