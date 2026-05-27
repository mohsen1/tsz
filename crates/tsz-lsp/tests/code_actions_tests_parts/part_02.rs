#[test]
fn test_extract_variable_math_max_call() {
    let source = "const result = Math.max(a, b);";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "Math.max(a, b)");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert!(
        actions.iter().any(|a| a.title.contains("Extract")),
        "Should offer extract for Math.max call"
    );
}

#[test]
fn test_code_actions_empty_range() {
    let source = "const x = 1;\nconst y = 2;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );
    let _ = actions;
}

#[test]
fn test_code_actions_on_class_declaration() {
    let source = "class Foo {\n  x: number = 0;\n  method() {}\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let range = range_for_substring(source, &line_map, "class Foo");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );
    let _ = actions;
}

#[test]
fn test_code_actions_on_interface() {
    let source = "interface Bar {\n  x: number;\n  y: string;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let range = range_for_substring(source, &line_map, "interface Bar");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );
    let _ = actions;
}

#[test]
fn test_code_actions_on_arrow_function_body() {
    let source = "const add = (a: number, b: number) => a + b;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let range = range_for_substring(source, &line_map, "a + b");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );
    let _ = actions;
}

#[test]
fn test_code_actions_on_empty_source() {
    let source = "";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );
    assert!(
        !actions
            .iter()
            .any(|a| a.title.starts_with("Extract to constant")),
        "Empty source should produce no extract variable actions"
    );
}

#[test]
fn test_code_actions_on_enum_declaration() {
    let source = "enum Color {\n  Red,\n  Green,\n  Blue\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let range = range_for_substring(source, &line_map, "enum Color");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );
    let _ = actions;
}

// =============================================================================
// addMissingAwait quick-fix: enclosing-context gating (issue #8762)
//
// Structural rule: the codefix may only fire when an `await` expression would
// be syntactically legal at the diagnostic position. The innermost enclosing
// function-like decides — a non-async function, non-async generator,
// non-async arrow, non-async method, constructor, getter, or setter all
// reject `await`, while async function-likes, class static blocks, and the
// top-level module body accept it.
// =============================================================================

fn run_add_missing_await_quickfix(source: &str, needle: &str) -> Option<CodeAction> {
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let range = range_for_substring(source, &line_map, needle);
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        source: None,
        message: "Property 'toString' does not exist on type 'Promise<number>'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };
    let actions = provider.provide_code_actions(
        root,
        Range::new(Position::new(0, 0), Position::new(0, 0)),
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );
    actions
        .into_iter()
        .find(|a| a.title == crate::code_actions::ADD_MISSING_AWAIT_TITLE)
}

#[track_caller]
fn assert_add_await_offered(source: &str, needle: &str) {
    assert!(
        run_add_missing_await_quickfix(source, needle).is_some(),
        "expected addMissingAwait to be offered; source:\n{source}",
    );
}

#[track_caller]
fn assert_add_await_not_offered(source: &str, needle: &str) {
    assert!(
        run_add_missing_await_quickfix(source, needle).is_none(),
        "expected addMissingAwait NOT to be offered; source:\n{source}",
    );
}

#[test]
fn add_missing_await_blocked_inside_non_async_generator() {
    let source =
        "function* g() {\n  const p: Promise<number> = fetchN();\n  return p.toString();\n}\n";
    assert_add_await_not_offered(source, "p.toString");
}

#[test]
fn add_missing_await_blocked_inside_non_async_generator_renamed_binder() {
    // Same shape with a renamed identifier — proves the rule is structural
    // and not keyed on the identifier names that happen to appear in the
    // test corpus.
    let source = "function* gen() {\n  const promise: Promise<number> = makeIt();\n  return promise.toString();\n}\n";
    assert_add_await_not_offered(source, "promise.toString");
}

#[test]
fn add_missing_await_blocked_inside_non_async_function() {
    let source =
        "function f() {\n  const p: Promise<number> = fetchN();\n  return p.toString();\n}\n";
    assert_add_await_not_offered(source, "p.toString");
}

#[test]
fn add_missing_await_blocked_inside_non_async_method() {
    let source = "class C {\n  m() {\n    const p: Promise<number> = fetchN();\n    return p.toString();\n  }\n}\n";
    assert_add_await_not_offered(source, "p.toString");
}

#[test]
fn add_missing_await_blocked_inside_constructor() {
    let source = "class C {\n  constructor() {\n    const p: Promise<number> = fetchN();\n    p.toString();\n  }\n}\n";
    assert_add_await_not_offered(source, "p.toString");
}

#[test]
fn add_missing_await_blocked_inside_getter() {
    let source = "class C {\n  get x() {\n    const p: Promise<number> = fetchN();\n    return p.toString();\n  }\n}\n";
    assert_add_await_not_offered(source, "p.toString");
}

#[test]
fn add_missing_await_blocked_inside_setter() {
    let source = "class C {\n  set x(_v: number) {\n    const p: Promise<number> = fetchN();\n    p.toString();\n  }\n}\n";
    assert_add_await_not_offered(source, "p.toString");
}

#[test]
fn add_missing_await_blocked_inside_non_async_arrow() {
    let source =
        "const f = () => {\n  const p: Promise<number> = fetchN();\n  return p.toString();\n};\n";
    assert_add_await_not_offered(source, "p.toString");
}

#[test]
fn add_missing_await_blocked_inside_arrow_nested_in_async_function() {
    let source = "async function outer() {\n  const cb = () => {\n    const p: Promise<number> = fetchN();\n    return p.toString();\n  };\n  return cb;\n}\n";
    assert_add_await_not_offered(source, "p.toString");
}

#[test]
fn add_missing_await_offered_at_top_level() {
    let source = "const p: Promise<number> = fetchN();\np.toString();\n";
    assert_add_await_offered(source, "p.toString");
}

#[test]
fn add_missing_await_offered_inside_async_function() {
    let source =
        "async function f() {\n  const p: Promise<number> = fetchN();\n  return p.toString();\n}\n";
    assert_add_await_offered(source, "p.toString");
}

#[test]
fn add_missing_await_offered_inside_async_generator() {
    let source = "async function* ag() {\n  const p: Promise<number> = fetchN();\n  return p.toString();\n}\n";
    assert_add_await_offered(source, "p.toString");
}

#[test]
fn add_missing_await_offered_inside_async_method() {
    let source = "class C {\n  async m() {\n    const p: Promise<number> = fetchN();\n    return p.toString();\n  }\n}\n";
    assert_add_await_offered(source, "p.toString");
}

#[test]
fn add_missing_await_offered_inside_async_arrow() {
    let source = "const f = async () => {\n  const p: Promise<number> = fetchN();\n  return p.toString();\n};\n";
    assert_add_await_offered(source, "p.toString");
}

#[test]
fn add_missing_await_offered_inside_async_arrow_nested_in_non_async_function() {
    let source = "function outer() {\n  const cb = async () => {\n    const p: Promise<number> = fetchN();\n    return p.toString();\n  };\n  return cb;\n}\n";
    assert_add_await_offered(source, "p.toString");
}

#[test]
fn add_missing_await_not_offered_inside_class_static_block() {
    // `await` is syntactically invalid inside class static initialization
    // blocks (same restriction as class field initializers), so the fix
    // must not be offered there even when a nested async function is absent.
    let source = "class C {\n  static {\n    const p: Promise<number> = fetchN();\n    p.toString();\n  }\n}\n";
    assert_add_await_not_offered(source, "p.toString");
}
