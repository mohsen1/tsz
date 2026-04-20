#[test]
fn test_rename_multiple_occurrences_same_line() {
    let source = "const x = 1; const y = x + x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "z".to_string());
    assert!(result.is_ok(), "Should rename variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 3,
        "Should rename declaration + 2 usages, got {}",
        edits.len()
    );
}

#[test]
fn test_prepare_rename_on_string_literal() {
    let source = "const s = \"hello\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, _root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position inside string literal
    let range = provider.prepare_rename(Position::new(0, 12));
    assert!(
        range.is_none(),
        "Should not allow renaming inside a string literal"
    );
}

#[test]
fn test_rename_class_method() {
    let source = "class Foo {\n  bar() {}\n}\nconst f = new Foo();\nf.bar();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on 'bar' method declaration (line 1, col 2)
    let range = provider.prepare_rename(Position::new(1, 2));
    assert!(
        range.is_some(),
        "Should be able to prepare rename for method"
    );
}

// =========================================================================
// Additional edge-case tests
// =========================================================================

#[test]
fn test_rename_namespace_name() {
    let source = "namespace MyNS {\n  export const val = 1;\n}\nMyNS.val;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 10), "NS".to_string());
    assert!(result.is_ok(), "Should rename namespace");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename namespace declaration + usage"
    );
    for e in edits {
        assert_eq!(e.new_text, "NS");
    }
}

#[test]
fn test_rename_arrow_function_param() {
    let source = "const fn = (x: number) => x * 2;\nfn(3);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'x' parameter (col 12)
    let result = provider.provide_rename_edits(root, Position::new(0, 12), "val".to_string());
    assert!(result.is_ok(), "Should rename arrow function param");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename param declaration + usage in body"
    );
    for e in edits {
        assert_eq!(e.new_text, "val");
    }
}

#[test]
fn test_prepare_rename_number_literal_returns_none() {
    let source = "const x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at numeric literal '42' (col 10)
    let range = provider.prepare_rename(Position::new(0, 10));
    assert!(range.is_none(), "Should not allow renaming number literal");
}

#[test]
fn test_rename_for_loop_variable() {
    let source = "for (let i = 0; i < 10; i++) {\n  console.log(i);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'i' declaration (col 9)
    let result = provider.provide_rename_edits(root, Position::new(0, 9), "idx".to_string());
    assert!(result.is_ok(), "Should rename for loop variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename loop variable across usages"
    );
    for e in edits {
        assert_eq!(e.new_text, "idx");
    }
}

#[test]
fn test_rename_catch_clause_variable() {
    let source = "try {\n  throw 1;\n} catch (err) {\n  console.log(err);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'err' in catch clause (line 2, col 9)
    let result = provider.provide_rename_edits(root, Position::new(2, 9), "error".to_string());
    assert!(result.is_ok(), "Should rename catch clause variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename catch variable declaration + usage"
    );
    for e in edits {
        assert_eq!(e.new_text, "error");
    }
}

#[test]
fn test_prepare_rename_empty_file_returns_none() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = provider.prepare_rename(Position::new(0, 0));
    assert!(
        range.is_none(),
        "Empty file should return None for prepare rename"
    );
}

#[test]
fn test_rename_class_name_with_constructor_usage() {
    let source =
        "class Animal {\n  constructor(public name: string) {}\n}\nconst a = new Animal('dog');";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "Pet".to_string());
    assert!(result.is_ok(), "Should rename class with constructor usage");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename class declaration + new expression"
    );
}

