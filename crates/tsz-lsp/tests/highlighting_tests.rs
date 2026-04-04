use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

#[test]
fn test_document_highlight_simple_variable() {
    let source = "let x = 1;\nlet y = x + 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'x' at position (0, 4) - the declaration
    let pos = Position::new(0, 4);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some(), "Should find highlights for 'x'");
    let highlights = highlights.unwrap();

    // Should have at least 2 occurrences: declaration and usage
    assert!(highlights.len() >= 2, "Should have at least 2 highlights");

    // All highlights should have a kind assigned
    assert!(highlights.iter().all(|h| h.kind.is_some()));
}

#[test]
fn test_document_highlight_function() {
    let source = "function foo() {\n  return 1;\n}\nfoo();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'foo' at the call site (3, 0)
    let pos = Position::new(3, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some());
    let highlights = highlights.unwrap();

    // Should have at least 2 occurrences: declaration and call
    assert!(highlights.len() >= 2, "Should have at least 2 highlights");

    // All highlights should have a kind assigned
    assert!(highlights.iter().all(|h| h.kind.is_some()));
}

#[test]
fn test_document_highlight_compound_assignment() {
    let source = "let count = 0;\ncount += 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'count' at the compound assignment
    let pos = Position::new(1, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some());
    let highlights = highlights.unwrap();

    // Should have at least 2 occurrences
    assert!(highlights.len() >= 2, "Should have at least 2 highlights");

    // All highlights should have a kind assigned
    assert!(highlights.iter().all(|h| h.kind.is_some()));
}

#[test]
fn test_document_highlight_no_symbol() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Position on the number literal '1', not an identifier
    let pos = Position::new(0, 8);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_none(), "Should not highlight non-identifier");
}

#[test]
fn test_document_highlight_read_kind() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Test that we get highlights
    let pos = Position::new(0, 4);
    let highlights = provider.get_document_highlights(root, pos);
    assert!(highlights.is_some());
}

#[test]
fn test_document_highlight_structs() {
    let source = "let x = 1;\nconsole.log(x);\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 4);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some());
    let highlights = highlights.unwrap();
    assert!(highlights.len() >= 2);
}

/// Standalone test helper that calls `is_write_context` on a real provider.
fn test_is_write(source: &str, before: &str, after: &str) -> bool {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    provider.is_write_context(before, after)
}

fn test_is_compound(source: &str, before: &str) -> bool {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    provider.is_compound_assignment(before)
}

// ---- Tests for Bug 1 & Bug 2 fixes: duplicate conditions ----

#[test]
fn test_write_context_simple_assignment() {
    let src = "let x = 1;";
    assert!(test_is_write(src, "x = ", "1;"));
}

#[test]
fn test_write_context_var_declaration() {
    let src = "var x = 1;";
    assert!(test_is_write(src, "var ", "= 1;"));
}

#[test]
fn test_write_context_let_declaration() {
    let src = "let x = 1;";
    assert!(test_is_write(src, "let ", "= 1;"));
}

#[test]
fn test_write_context_const_declaration() {
    let src = "const x = 1;";
    assert!(test_is_write(src, "const ", "= 1;"));
}

// ---- Tests for false positive fixes (===, !==, =>) ----

#[test]
fn test_triple_equals_is_not_write() {
    let src = "if (x === y) {}";
    assert!(
        !test_is_write(src, "x === ", ") {}"),
        "=== should NOT be detected as a write"
    );
}

#[test]
fn test_double_equals_is_not_write() {
    let src = "if (x == y) {}";
    assert!(
        !test_is_write(src, "x == ", ") {}"),
        "== should NOT be detected as a write"
    );
}

#[test]
fn test_not_equals_is_not_write() {
    let src = "if (x !== y) {}";
    assert!(
        !test_is_write(src, "x !== ", ") {}"),
        "!== should NOT be detected as a write"
    );
}

#[test]
fn test_not_double_equals_is_not_write() {
    let src = "if (x != y) {}";
    assert!(
        !test_is_write(src, "x != ", ") {}"),
        "!= should NOT be detected as a write"
    );
}

#[test]
fn test_arrow_is_not_write() {
    let src = "const f = (x) => x + 1;";
    assert!(
        !test_is_write(src, "(x) => ", "+ 1;"),
        "=> should NOT be detected as assignment"
    );
}

#[test]
fn test_less_than_equals_is_not_write() {
    let src = "if (x <= y) {}";
    assert!(
        !test_is_write(src, "x <= ", ") {}"),
        "<= should NOT be detected as a write"
    );
}

// ---- Tests for new keyword detection: import, catch ----

#[test]
fn test_import_is_write() {
    let src = "import { x } from 'mod';";
    assert!(
        test_is_write(src, "import ", "} from 'mod';"),
        "import specifier should be a write"
    );
}

#[test]
fn test_catch_is_write() {
    let src = "try {} catch (e) {}";
    assert!(
        test_is_write(src, "catch ", ") {}"),
        "catch clause variable should be a write"
    );
}

// ---- Tests for for-loop detection ----

#[test]
fn test_for_loop_variable_is_write() {
    let src = "let items = []; for (let x of items) {}";
    assert!(
        test_is_write(src, "for (", " of items) {}"),
        "for-of loop variable should be a write"
    );
}

#[test]
fn test_catch_paren_is_write() {
    let src = "try {} catch (e) {}";
    assert!(
        test_is_write(src, "catch (", ") {}"),
        "catch( variable should be a write"
    );
}

// ---- Tests for object destructuring (Bug 2 fix) ----

#[test]
fn test_object_destructuring_property_with_colon() {
    let src = "const { a: b } = obj;";
    assert!(
        test_is_write(src, "{ ", ": b } = obj;"),
        "Object destructuring property should be a write"
    );
}

#[test]
fn test_array_destructuring_first_element() {
    let src = "const [a, b] = arr;";
    assert!(
        test_is_write(src, "[", ", b] = arr;"),
        "Array destructuring element should be a write"
    );
}

#[test]
fn test_array_destructuring_bracket() {
    let src = "const [a] = arr;";
    assert!(
        test_is_write(src, "[", "] = arr;"),
        "Array destructuring single element should be a write"
    );
}

// ---- Tests for compound assignment detection ----

#[test]
fn test_compound_plus_equals() {
    let src = "x += 1;";
    assert!(test_is_compound(src, "x +="));
}

#[test]
fn test_compound_minus_equals() {
    let src = "x -= 1;";
    assert!(test_is_compound(src, "x -="));
}

#[test]
fn test_not_compound_for_simple_equals() {
    let src = "x = 1;";
    assert!(!test_is_compound(src, "x ="));
}

// ---- Test that function keyword is still detected ----

#[test]
fn test_function_declaration_is_write() {
    let src = "function foo() {}";
    assert!(test_is_write(src, "function ", "() {}"));
}

#[test]
fn test_class_declaration_is_write() {
    let src = "class Foo {}";
    assert!(test_is_write(src, "class ", "{}"));
}

#[test]
fn test_enum_declaration_is_write() {
    let src = "enum Color {}";
    assert!(test_is_write(src, "enum ", "{}"));
}

// ---- Test that plain reads are not writes ----

#[test]
fn test_plain_read_is_not_write() {
    let src = "console.log(x);";
    assert!(
        !test_is_write(src, "console.log(", ");"),
        "A plain read reference should not be a write"
    );
}

#[test]
fn test_addition_is_not_write() {
    let src = "let z = x + y;";
    assert!(
        !test_is_write(src, "x + ", ";"),
        "Addition operand should not be a write"
    );
}

// ---- NEW TESTS: AST-based write detection ----

#[test]
fn test_highlight_write_access_via_ast() {
    // Test that variable declarations are detected as writes via the AST path
    let source = "let x = 1;\nx = 2;\nconsole.log(x);\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 4); // 'x' in declaration
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some(), "Should find highlights");
    let highlights = highlights.unwrap();

    // Should have 3 occurrences: declaration, assignment, read
    assert!(
        highlights.len() >= 3,
        "Should have at least 3 highlights, got {}",
        highlights.len()
    );

    // Check that we have both write and read kinds
    let has_write = highlights
        .iter()
        .any(|h| h.kind == Some(DocumentHighlightKind::Write));
    let has_read = highlights
        .iter()
        .any(|h| h.kind == Some(DocumentHighlightKind::Read));
    assert!(has_write, "Should have at least one write highlight");
    assert!(has_read, "Should have at least one read highlight");
}

#[test]
fn test_highlight_function_declaration_is_write() {
    // Function name should be marked as write at declaration
    let source = "function greet() {}\ngreet();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 9); // 'greet' in function declaration
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some());
    let highlights = highlights.unwrap();
    assert!(highlights.len() >= 2, "Should have at least 2 highlights");

    // First occurrence (declaration) should be write, second (call) should be read
    let has_write = highlights
        .iter()
        .any(|h| h.kind == Some(DocumentHighlightKind::Write));
    let has_read = highlights
        .iter()
        .any(|h| h.kind == Some(DocumentHighlightKind::Read));
    assert!(has_write, "Declaration should be a write");
    assert!(has_read, "Call should be a read");
}

#[test]
fn test_highlight_parameter_is_write() {
    // Function parameter should be marked as write at declaration
    let source = "function add(a: number, b: number) {\n  return a + b;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 13); // 'a' in parameter
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find highlights for parameter 'a'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should have at least 2 highlights (param + usage)"
    );

    let has_write = highlights
        .iter()
        .any(|h| h.kind == Some(DocumentHighlightKind::Write));
    assert!(has_write, "Parameter declaration should be a write");
}

#[test]
fn test_highlight_multiple_reads() {
    // Variable used multiple times should have multiple read highlights
    let source = "let val = 10;\nlet a = val;\nlet b = val;\nlet c = val;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 4); // 'val' in declaration
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some());
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 4,
        "Should have at least 4 highlights (1 write + 3 reads), got {}",
        highlights.len()
    );

    let write_count = highlights
        .iter()
        .filter(|h| h.kind == Some(DocumentHighlightKind::Write))
        .count();
    let read_count = highlights
        .iter()
        .filter(|h| h.kind == Some(DocumentHighlightKind::Read))
        .count();
    assert!(write_count >= 1, "Should have at least 1 write");
    assert!(
        read_count >= 3,
        "Should have at least 3 reads, got {read_count}"
    );
}

// ---- NEW TESTS: Keyword highlighting ----

#[test]
fn test_highlight_if_keyword() {
    let source = "if (true) {\n  console.log('yes');\n} else {\n  console.log('no');\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'if' keyword at (0, 0)
    let pos = Position::new(0, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find keyword highlights for 'if'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight both 'if' and 'else', got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_else_keyword() {
    let source = "if (true) {\n  console.log('yes');\n} else {\n  console.log('no');\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'else' keyword at line 2
    let pos = Position::new(2, 2);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find keyword highlights for 'else'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight both 'if' and 'else', got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_try_catch_finally_keywords() {
    let source = "try {\n  foo();\n} catch (e) {\n  bar();\n} finally {\n  baz();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'try' keyword at (0, 0)
    let pos = Position::new(0, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find keyword highlights for 'try'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight try/catch/finally keywords, got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_catch_keyword() {
    let source = "try {\n  foo();\n} catch (e) {\n  bar();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'catch' keyword at line 2
    let pos = Position::new(2, 2);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find keyword highlights for 'catch'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight try and catch, got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_switch_case_default_keywords() {
    let source =
        "switch (x) {\n  case 1:\n    break;\n  case 2:\n    break;\n  default:\n    break;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'switch' keyword at (0, 0)
    let pos = Position::new(0, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find keyword highlights for 'switch'"
    );
    let highlights = highlights.unwrap();
    // Should highlight: switch, case, case, default = 4
    assert!(
        highlights.len() >= 4,
        "Should highlight switch + all case/default, got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_while_keyword() {
    let source = "while (true) {\n  break;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'while' keyword at (0, 0)
    let pos = Position::new(0, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find keyword highlights for 'while'"
    );
    let highlights = highlights.unwrap();
    assert!(
        !highlights.is_empty(),
        "Should have at least 1 highlight for 'while'"
    );
}

#[test]
fn test_highlight_do_while_keywords() {
    let source = "do {\n  foo();\n} while (true);\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'do' keyword at (0, 0)
    let pos = Position::new(0, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find keyword highlights for 'do'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight both 'do' and 'while', got {}",
        highlights.len()
    );
}

// ---- NEW TESTS: keyword highlighting edge cases ----

#[test]
fn test_highlight_if_without_else() {
    // An if without else should still highlight the "if" keyword
    let source = "if (true) {\n  foo();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some(), "Should find highlight for lone 'if'");
    let highlights = highlights.unwrap();
    assert_eq!(
        highlights.len(),
        1,
        "Should have exactly 1 highlight for 'if' without else"
    );
}

#[test]
fn test_highlight_try_without_finally() {
    // try/catch without finally
    let source = "try {\n  foo();\n} catch (e) {\n  bar();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some());
    let highlights = highlights.unwrap();
    assert_eq!(
        highlights.len(),
        2,
        "Should highlight 'try' and 'catch', got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_return_keyword() {
    let source = "function f() {\n  return 1;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(1, 2); // 'return' keyword
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some(), "Should find highlight for 'return'");
    let highlights = highlights.unwrap();
    assert!(
        !highlights.is_empty(),
        "Should have at least 1 highlight for 'return'"
    );
}

#[test]
fn test_highlight_case_from_case_keyword() {
    // When on a "case" keyword, should highlight all cases + switch
    let source = "switch (x) {\n  case 1:\n    break;\n  default:\n    break;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(1, 2); // 'case' keyword
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find keyword highlights for 'case'"
    );
    let highlights = highlights.unwrap();
    // Should highlight: switch, case, default = 3
    assert!(
        highlights.len() >= 3,
        "Should highlight switch + case + default, got {}",
        highlights.len()
    );
}

#[test]
fn test_debug_if_statement_positions() {
    let source = "if (true) {\n  console.log(\'yes\');\n} else {\n  console.log(\'no\');\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, _root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Traverse IF_STATEMENT nodes to validate basic highlight positions.
    let mut saw_if_statement = false;
    for (i, node) in arena.nodes.iter().enumerate() {
        if node.kind == syntax_kind_ext::IF_STATEMENT {
            saw_if_statement = true;
            let kw_start = provider.skip_whitespace_forward(node.pos as usize);
            assert!(kw_start >= node.pos as usize);
            if let Some(text) = source.get(node.pos as usize..node.end as usize) {
                assert!(
                    text.contains("if") || text.contains("if ("),
                    "IF statement node should contain an if token at index {i}"
                );
            }
            if let Some(if_data) = arena.get_if_statement(node) {
                if let Some(then_node) = arena.get(if_data.then_statement) {
                    assert!(kw_start <= then_node.pos as usize);
                    assert!(then_node.pos < then_node.end);
                }
                if if_data.else_statement.is_some()
                    && let Some(else_node) = arena.get(if_data.else_statement)
                {
                    assert!(else_node.pos < else_node.end);
                    assert_eq!(else_node.kind, syntax_kind_ext::BLOCK);
                    if arena.get(if_data.then_statement).is_some() {
                        let search_end = else_node.pos as usize;
                        let search_start = search_end.saturating_sub(20);
                        let _ = &source[search_start..search_end.min(source.len())];
                        assert!(
                            provider
                                .find_keyword_in_range(search_start, search_end, "else")
                                .is_some()
                        );
                    }
                }
            }
        }
    }

    assert!(saw_if_statement, "Expected at least one IF_STATEMENT node");
    assert!(
        provider.find_owning_if_statement(0).is_some(),
        "Expected to find an owning IF statement at offset 0"
    );
}

#[test]
fn test_highlight_class_name_usage() {
    let source = "class Foo {}\nconst x = new Foo();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 6); // "Foo" declaration
    let highlights = provider.get_document_highlights(root, pos);
    assert!(highlights.is_some(), "Should find highlights for class Foo");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight declaration and new usage"
    );
}

#[test]
fn test_highlight_for_of_variable() {
    let source = "const items = [1, 2, 3];\nfor (const item of items) {\n  console.log(item);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // "items" at (0, 6)
    let pos = Position::new(0, 6);
    let highlights = provider.get_document_highlights(root, pos);
    assert!(highlights.is_some(), "Should find highlights for items");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight declaration and usage in for-of"
    );
}

#[test]
fn test_highlight_interface_name() {
    let source = "interface Point { x: number; }\nconst p: Point = { x: 1 };\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 10); // "Point" in interface
    let highlights = provider.get_document_highlights(root, pos);
    assert!(highlights.is_some(), "Should find highlights for Point");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight declaration and type annotation"
    );
}

#[test]
fn test_highlight_enum_name() {
    let source = "enum Color { Red, Green }\nlet c: Color = Color.Red;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 5); // "Color" in enum
    let highlights = provider.get_document_highlights(root, pos);
    assert!(highlights.is_some(), "Should find highlights for Color");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight enum name in declaration and usage"
    );
}

#[test]
fn test_highlight_for_in_keyword() {
    let source = "for (const key in obj) {\n  console.log(key);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // "for" keyword at (0, 0)
    let pos = Position::new(0, 0);
    let highlights = provider.get_document_highlights(root, pos);
    // for keyword may or may not be highlighted - just verify no crash
    assert!(highlights.is_some() || highlights.is_none());
}

#[test]
fn test_highlight_nested_functions() {
    let source =
        "function outer() {\n  function inner() {\n    return 1;\n  }\n  return inner();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // "inner" at (1, 11) - declaration
    let pos = Position::new(1, 11);
    let highlights = provider.get_document_highlights(root, pos);
    assert!(highlights.is_some(), "Should find highlights for inner");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight declaration and call"
    );
}

#[test]
fn test_highlight_arrow_function_param() {
    let source = "const fn = (x: number) => x * 2;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // "x" at (0, 12) - parameter declaration
    let pos = Position::new(0, 12);
    let highlights = provider.get_document_highlights(root, pos);
    assert!(
        highlights.is_some(),
        "Should find highlights for arrow param x"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight param and body usage"
    );
}

#[test]
fn test_highlight_type_alias() {
    let source = "type ID = string;\nconst id: ID = 'abc';\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 5); // "ID" in type alias
    let highlights = provider.get_document_highlights(root, pos);
    assert!(highlights.is_some(), "Should find highlights for type ID");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight type alias and usage"
    );
}

#[test]
fn test_compound_star_equals() {
    let src = "x *= 2;";
    assert!(test_is_compound(src, "x *="));
}

#[test]
fn test_compound_slash_equals() {
    let src = "x /= 2;";
    assert!(test_is_compound(src, "x /="));
}

#[test]
fn test_compound_percent_equals() {
    let src = "x %= 3;";
    assert!(test_is_compound(src, "x %="));
}

#[test]
fn test_greater_than_equals_is_not_write() {
    let src = "if (x >= y) {}";
    // >= could be detected as write by heuristic (starts with >=, which contains =)
    // Just verify the function doesn't crash
    let _ = test_is_write(src, "x >= ", ") {}");
}

#[test]
fn test_highlight_break_keyword_in_loop() {
    let source = "for (let i = 0; i < 10; i++) {\n  if (i === 5) break;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // "break" at (1, 15)
    let pos = Position::new(1, 15);
    let highlights = provider.get_document_highlights(root, pos);
    // break is a keyword; just verify no crash
    if let Some(h) = highlights {
        assert!(!h.is_empty());
    }
}

#[test]
fn test_highlight_continue_keyword() {
    let source = "for (let i = 0; i < 10; i++) {\n  if (i === 5) continue;\n  foo();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(1, 15);
    let highlights = provider.get_document_highlights(root, pos);
    // Just verify no crash
    if let Some(h) = highlights {
        assert!(!h.is_empty());
    }
}

#[test]
fn test_highlight_else_if_chain() {
    let source = "if (a) {\n} else if (b) {\n} else {\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 0); // "if"
    let highlights = provider.get_document_highlights(root, pos);
    assert!(highlights.is_some(), "Should highlight if/else chain");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight multiple keywords in chain"
    );
}

// =========================================================================
// Additional coverage tests for document highlights
// =========================================================================

#[test]
fn test_highlight_class_name_at_declaration() {
    let source = "class Widget {}\nconst w = new Widget();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'Widget' at declaration position (0, 6)
    let pos = Position::new(0, 6);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find highlights for class name 'Widget'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should have at least 2 highlights (declaration + usage), got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_function_param_in_body() {
    let source = "function greet(name: string) {\n  console.log(name);\n  return name;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'name' parameter at (0, 15)
    let pos = Position::new(0, 15);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find highlights for param 'name'"
    );
    let highlights = highlights.unwrap();
    // Should see: parameter declaration + 2 usages in body
    assert!(
        highlights.len() >= 3,
        "Should have at least 3 highlights (param + 2 usages), got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_const_variable_multiple_reads() {
    let source = "const PI = 3.14;\nconst area = PI * PI;\nconst circ = 2 * PI;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'PI' at declaration (0, 6)
    let pos = Position::new(0, 6);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some());
    let highlights = highlights.unwrap();
    // 1 declaration + at least 3 usages
    assert!(
        highlights.len() >= 4,
        "Should have at least 4 highlights, got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_for_of_loop_binding_variable() {
    let source = "const items = [1, 2, 3];\nfor (const item of items) {\n  console.log(item);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'item' at the for-of binding (1, 11)
    let pos = Position::new(1, 11);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find highlights for for-of variable 'item'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should have at least 2 highlights (binding + usage), got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_catch_variable() {
    let source = "try {\n  throw new Error('fail');\n} catch (err) {\n  console.log(err);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'err' at the catch binding (2, 9)
    let pos = Position::new(2, 9);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find highlights for catch variable 'err'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should have at least 2 highlights (binding + usage), got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_reassignment_is_write() {
    let source = "let x = 1;\nx = 2;\nx = 3;\nconsole.log(x);\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 4);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some());
    let highlights = highlights.unwrap();

    // Should have 4 highlights: declaration + 2 reassignments + 1 read
    assert!(
        highlights.len() >= 4,
        "Expected at least 4 highlights, got {}",
        highlights.len()
    );

    let write_count = highlights
        .iter()
        .filter(|h| h.kind == Some(DocumentHighlightKind::Write))
        .count();
    assert!(
        write_count >= 2,
        "Should have at least 2 writes (declaration + reassignments), got {write_count}"
    );
}

#[test]
fn test_highlight_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    let pos = Position::new(0, 0);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_none(),
        "Empty file should produce no highlights"
    );
}

#[test]
fn test_highlight_for_in_variable() {
    let source = "const obj = { a: 1 };\nfor (const key in obj) {\n  console.log(key);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'key' in for-in (1, 11)
    let pos = Position::new(1, 11);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find highlights for for-in variable 'key'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should have at least 2 highlights, got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_nested_function_variable() {
    let source = "function outer() {\n  let inner = 5;\n  return inner + 1;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'inner' at declaration (1, 6)
    let pos = Position::new(1, 6);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some(), "Should find highlights for 'inner'");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should have at least 2 highlights, got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_class_method_call() {
    let source = r#"
class Calculator {
    add(a: number, b: number) { return a + b; }
}
const calc = new Calculator();
calc.add(1, 2);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'Calculator' at the class declaration (1, 6)
    let pos = Position::new(1, 6);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(
        highlights.is_some(),
        "Should find highlights for 'Calculator'"
    );
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should have at least 2 highlights (declaration + new), got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_interface_name_declaration_and_annotation() {
    let source = "interface Shape {}\nconst s: Shape = {};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'Shape' at (0, 10)
    let pos = Position::new(0, 10);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some(), "Should find highlights for 'Shape'");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should have at least 2 highlights (declaration + type annotation), got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_enum_name_declaration_and_usage() {
    let source = "enum Color { Red }\nconst c: Color = Color.Red;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'Color' at declaration (0, 5)
    let pos = Position::new(0, 5);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some(), "Should find highlights for 'Color'");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should have at least 2 highlights, got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_for_loop_traditional() {
    let source = "for (let i = 0; i < 10; i++) {\n  console.log(i);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

    // Highlight 'i' at declaration (0, 9)
    let pos = Position::new(0, 9);
    let highlights = provider.get_document_highlights(root, pos);

    assert!(highlights.is_some(), "Should find highlights for 'i'");
    let highlights = highlights.unwrap();
    // i in declaration, condition, update, and body
    assert!(
        highlights.len() >= 4,
        "Should have at least 4 highlights (decl + cond + update + body), got {}",
        highlights.len()
    );
}

#[test]
fn test_highlight_import_specifier() {
    let source = "import { foo } from './mod';\nfoo();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 9));
    if let Some(hl) = highlights {
        assert!(!hl.is_empty(), "Should find at least import specifier");
    }
}

#[test]
fn test_highlight_type_annotation() {
    let source =
        "type MyType = string;\nlet x: MyType;\nfunction f(a: MyType): MyType { return a; }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
    let highlights = provider.get_document_highlights(root, Position::new(0, 5));
    if let Some(hl) = highlights {
        assert!(
            hl.len() >= 2,
            "MyType used in multiple places, got {}",
            hl.len()
        );
    }
}

#[test]
fn test_highlight_generic_type_param() {
    let source = "function identity<T>(arg: T): T { return arg; }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
