#[test]
fn test_find_references_function_used_as_callback() {
    let source = "function handler() {}\nconst arr = [1, 2];\narr.forEach(handler);";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'handler' declaration (line 0, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 9));

    assert!(
        refs.is_some(),
        "Should find references for function used as callback"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find function declaration + callback usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_default_parameter() {
    let source = "function greet(name = 'world') { return name; }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 15));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find name param + usage");
    }
}

#[test]
fn test_find_references_computed_property_name() {
    let source = "const key = 'x';\nconst obj = { [key]: 1 };";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find key decl + computed usage");
    }
}

#[test]
fn test_find_references_switch_case_variable() {
    let source = "const x = 1;\nswitch(x) { case 0: break; default: x; }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_class_constructor_param() {
    let source = "class Foo {\n  constructor(public x: number) {}\n  get() { return this.x; }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 22));
    let _ = refs;
}

#[test]
fn test_find_references_spread_element() {
    let source = "const arr = [1, 2];\nconst copy = [...arr];";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find arr decl + spread usage");
    }
}

#[test]
fn test_find_references_typeof_expression() {
    let source = "const x = 42;\ntype T = typeof x;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(!r.is_empty());
    }
}

#[test]
fn test_find_references_optional_chaining_variable() {
    let source = "const obj = { a: 1 };\nconst val = obj?.a;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_nullish_coalescing_variable() {
    let source = "const x = null;\nconst y = x ?? 'default';";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_multiple_declarations_same_name() {
    let source =
        "function foo() { const x = 1; return x; }\nfunction bar() { const x = 2; return x; }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at x in foo
    let refs = find_refs.find_references(root, Position::new(0, 23));
    let _ = refs;
}

#[test]
fn test_find_references_export_assignment() {
    let source = "const value = 42;\nexport default value;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + export usage");
    }
}

#[test]
fn test_find_references_shorthand_property() {
    let source = "const x = 1;\nconst obj = { x };";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + shorthand usage");
    }
}

#[test]
fn test_find_references_class_static_property() {
    let source = "class Foo {\n  static count = 0;\n  inc() { Foo.count++; }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    let _ = refs;
}

#[test]
fn test_detailed_refs_let_reassignment_is_write() {
    let source = "let x = 1;\nx = 2;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);
    let writes: Vec<_> = refs.iter().filter(|r| r.is_write_access).collect();
    assert!(!writes.is_empty(), "Reassignment should be a write");
}

#[test]
fn test_detailed_refs_delete_expression() {
    let source = "const obj: any = { x: 1 };\ndelete obj.x;";
    let refs = get_detailed_refs(source, "test.ts", 0, 6);
    assert!(!refs.is_empty());
}

// =========================================================================
// Additional reference tests (batch 4 — edge cases)
// =========================================================================

#[test]
fn test_find_references_single_char_identifier() {
    let source = "const a = 1;\na;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(
            r.len() >= 2,
            "Should find declaration + usage for single-char id"
        );
    }
}

#[test]
fn test_find_references_unicode_identifier() {
    let source = "const \u{00e4}\u{00f6}\u{00fc} = 1;\n\u{00e4}\u{00f6}\u{00fc};";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    let _ = refs;
}

#[test]
fn test_find_references_let_in_block_scope() {
    let source = "{ let y = 10; y; }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find block-scoped let refs");
    }
}

#[test]
fn test_find_references_var_in_function() {
    let source = "function f() { var v = 1; return v; }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 19));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find var decl + return usage");
    }
}

#[test]
fn test_find_references_ternary_condition() {
    let source = "const flag = true;\nconst result = flag ? 'yes' : 'no';";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(
            r.len() >= 2,
            "Should find flag decl + ternary condition usage"
        );
    }
}

#[test]
fn test_find_references_while_loop_condition() {
    let source = "let running = true;\nwhile (running) { running = false; }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 3, "Should find decl + condition + assignment");
    }
}

#[test]
fn test_find_references_do_while_condition() {
    let source = "let count = 0;\ndo { count++; } while (count < 5);";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_nested_destructuring() {
    let source = "const { a: { b } } = { a: { b: 42 } };\nb;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 0));
    let _ = refs;
}

#[test]
fn test_find_references_class_private_field() {
    let source = "class Foo {\n  #secret = 42;\n  get() { return this.#secret; }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 2));
    let _ = refs;
}

#[test]
fn test_find_references_async_function_name() {
    let source = "async function fetchData() {}\nawait fetchData();";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 15));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find async function decl + call");
    }
}

#[test]
fn test_find_references_generator_function_name() {
    let source = "function* gen() { yield 1; }\nconst it = gen();";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find generator decl + call");
    }
}

#[test]
fn test_find_references_type_parameter_in_function() {
    let source = "function identity<T>(x: T): T { return x; }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 18));
    let _ = refs;
}

#[test]
fn test_find_references_type_parameter_in_class() {
    let source = "class Container<T> {\n  value: T;\n  get(): T { return this.value; }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 16));
    let _ = refs;
}

#[test]
fn test_find_references_comma_operator() {
    let source = "let x = 0;\n(x++, x);";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_logical_assignment() {
    let source = "let x: number | null = null;\nx ??= 42;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_in_arrow_return_expression() {
    let source = "const val = 10;\nconst fn = () => val;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + arrow return usage");
    }
}

#[test]
fn test_find_references_in_object_spread() {
    let source = "const base = { a: 1 };\nconst ext = { ...base, b: 2 };";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + object spread usage");
    }
}

#[test]
fn test_find_references_in_array_index() {
    let source = "const idx = 0;\nconst arr = [1, 2, 3];\narr[idx];";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find idx decl + element access usage");
    }
}

#[test]
fn test_find_references_in_if_condition() {
    let source = "const cond = true;\nif (cond) { }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + if-condition usage");
    }
}

#[test]
fn test_find_references_class_method_name() {
    let source = "class A {\n  run() {}\n}\nnew A().run();";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 2));
    let _ = refs;
}

#[test]
fn test_find_references_multiline_string_variable() {
    let source = "const msg = `line1\nline2\nline3`;\nconsole.log(msg);";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(
            r.len() >= 2,
            "Should find decl + log usage across multiline template"
        );
    }
}

#[test]
fn test_detailed_refs_for_loop_counter_is_write() {
    let source = "for (let i = 0; i < 10; i++) { i; }";
    let refs = get_detailed_refs(source, "test.ts", 0, 9);
    let writes: Vec<_> = refs.iter().filter(|r| r.is_write_access).collect();
    assert!(
        !writes.is_empty(),
        "for-loop init and increment should be writes"
    );
}
