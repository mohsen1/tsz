#[test]
fn test_highlight_generic_type_param() {
    let source = "function identity<T>(arg: T): T { return arg; }\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    // T at position (0, 18)
    let highlights = provider.get_document_highlights(root, Position::new(0, 18));
    if let Some(hl) = highlights {
        assert!(
            hl.len() >= 2,
            "T used in param and return type, got {}",
            hl.len()
        );
    }
}

#[test]
fn test_highlight_namespace_variable() {
    let source = "namespace NS {\n  export const val = 1;\n}\nconst x = NS.val;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 10));
    if let Some(hl) = highlights {
        assert!(!hl.is_empty(), "Should highlight NS");
    }
}

#[test]
fn test_highlight_computed_property() {
    let source = "const key = 'name';\nconst obj = { [key]: 'value' };\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 6));
    if let Some(hl) = highlights {
        assert!(
            hl.len() >= 2,
            "key used in declaration and computed property"
        );
    }
}

#[test]
fn test_highlight_spread_operator_variable() {
    let source = "const arr = [1, 2, 3];\nconst newArr = [...arr, 4];\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 6));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 2, "arr used in declaration and spread");
    }
}

#[test]
fn test_highlight_ternary_variable() {
    let source = "const flag = true;\nconst val = flag ? 'yes' : 'no';\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 6));
    if let Some(hl) = highlights {
        assert!(
            hl.len() >= 2,
            "flag used in declaration and ternary condition"
        );
    }
}

#[test]
fn test_highlight_optional_chaining_variable() {
    let source = "const obj = { a: { b: 1 } };\nconst val = obj?.a?.b;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 6));
    if let Some(hl) = highlights {
        assert!(
            hl.len() >= 2,
            "obj used in declaration and optional chaining"
        );
    }
}

#[test]
fn test_highlight_template_string_variable() {
    let source = "const name = 'World';\nconst msg = `Hello ${name}!`;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 6));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 2, "name used in declaration and template");
    }
}

#[test]
fn test_highlight_no_match_at_whitespace() {
    let source = "const x = 1;\n\nconst y = 2;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(1, 0));
    // Whitespace position should return None or empty
    if let Some(hl) = highlights {
        let _ = hl;
    }
}

#[test]
fn test_highlight_class_name_multiple_uses() {
    let source = "class Foo {}\nconst a = new Foo();\nconst b: Foo = a;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 6));
    if let Some(hl) = highlights {
        assert!(
            hl.len() >= 2,
            "Foo used in class decl + new + type annotation"
        );
    }
}

#[test]
fn test_highlight_enum_member() {
    let source = "enum Color { Red, Green, Blue }\nconst c = Color.Red;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 5));
    if let Some(hl) = highlights {
        assert!(!hl.is_empty());
    }
}

#[test]
fn test_highlight_for_loop_variable() {
    let source = "for (let i = 0; i < 10; i++) { console.log(i); }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 9));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 3, "i in init + condition + increment + body");
    }
}

#[test]
fn test_highlight_default_export() {
    let source = "export default function foo() {}\nfoo();";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 24));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 2);
    }
}

#[test]
fn test_highlight_destructured_variable() {
    let source = "const { x, y } = { x: 1, y: 2 };\nconsole.log(x + y);";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 8));
    if let Some(hl) = highlights {
        assert!(!hl.is_empty());
    }
}

#[test]
fn test_highlight_catch_parameter() {
    let source = "try { throw 1; } catch (err) { console.log(err); }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 24));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 2, "err in catch + usage");
    }
}

#[test]
fn test_highlight_interface_name_in_object_literal() {
    let source = "interface Foo { x: number; }\nconst a: Foo = { x: 1 };";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 10));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 2, "Foo in interface + type annotation");
    }
}

#[test]
fn test_highlight_empty_source() {
    let source = "";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 0));
    let _ = highlights;
}

#[test]
fn test_highlight_let_reassignment() {
    let source = "let x = 1;\nx = 2;\nx = 3;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 4));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 3, "x in decl + two reassignments");
    }
}

#[test]
fn test_highlight_arrow_function_param_in_body() {
    let source = "const fn = (a: number, b: number) => a + b;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 12));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 2, "a in param + body");
    }
}

#[test]
fn test_highlight_type_alias_id() {
    let source = "type ID = string;\nconst x: ID = 'abc';";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 5));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 2, "ID in type alias + annotation");
    }
}

#[test]
fn test_highlight_async_function_name() {
    let source = "async function fetchData() { return 1; }\nfetchData();";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 15));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 2, "fetchData in decl + call");
    }
}

#[test]
fn test_highlight_static_method() {
    let source = "class Foo {\n  static bar() {}\n}\nFoo.bar();";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 6));
    if let Some(hl) = highlights {
        assert!(hl.len() >= 2);
    }
}

/// Regression test: `walk_to_node` and `walk_for_scope` must not stack-overflow
/// on deeply-nested class hierarchies (e.g. long inherited-property chains).
///
/// Before the fix both recursive walkers lacked `stacker::maybe_grow` guards,
/// so an AST whose depth exceeded the default stack size would SIGABRT
/// (as seen in the fourslash `documentHighlightAtInheritedProperties6` test).
#[test]
fn test_highlight_deep_inheritance_no_stack_overflow() {
    // Build a chain: A0 extends A1, A1 extends A2, ... A{N-1} extends A{N}
    // Each class has a shared method `run()` to give `collect_member_access_reference_nodes`
    // something to walk back to root for.  N=60 produces a genuinely deep AST.
    let n = 60usize;
    let mut source = String::new();
    // Base class
    source.push_str(&format!("class A{n} {{ run(): void {{}} }}\n"));
    for i in (0..n).rev() {
        source.push_str(&format!("class A{i} extends A{} {{ }}\n", i + 1));
    }
    // Instantiate each and call the inherited method so there are many
    // property-access expressions for `collect_member_access_reference_nodes`.
    for i in 0..=n {
        source.push_str(&format!("const o{i} = new A{i}();\no{i}.run();\n"));
    }

    let (parser, root) = parse_test_source(&source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(&source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, &source);

    // Highlight `run` in the base class declaration (line 0, col ~12).
    // The exact column doesn't matter — the test just must not panic/SIGABRT.
    let _ = provider.get_document_highlights(root, Position::new(0, 12));
}

/// Regression test: circular class inheritance must not cause a stack overflow.
///
/// `class C extends D` + `class D extends C` creates a mutually recursive type
/// relationship. Requesting document highlights on `prop1` in either class must
/// complete without overflowing the stack (SIGABRT), regardless of which marker
/// position the cursor is at.
///
/// This mirrors `documentHighlightAtInheritedProperties6.ts` from the fourslash
/// corpus, which crashes the tsz-server process with a stack overflow on the
/// main thread when highlights are requested in the presence of circular class
/// inheritance.
#[test]
fn test_highlight_circular_class_inheritance_no_stack_overflow() {
    let source = r#"class C extends D {
    prop0: string;
    prop1: string;
}

class D extends C {
    prop0: string;
    prop1: string;
}

var d: D;
d.prop1;
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Request highlights from every [|prop1|] marker position — mirrors what
    // verify.baselineDocumentHighlights() does in fourslash test 6:
    //   line 1, col 4  → prop1 in class C
    //   line 6, col 4  → prop1 in class D
    //   line 11, col 2 → d.prop1 (the access)
    for (line, col) in [(1u32, 4u32), (6u32, 4u32), (11u32, 2u32)] {
        let _ = provider.get_document_highlights(root, Position::new(line, col));
    }
}
