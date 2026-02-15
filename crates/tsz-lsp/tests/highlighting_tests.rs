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
        "Should have at least 3 reads, got {}",
        read_count
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

    // Print all IF_STATEMENT nodes and their positions
    for (i, node) in arena.nodes.iter().enumerate() {
        if node.kind == syntax_kind_ext::IF_STATEMENT {
            println!(
                "IF_STATEMENT at index {}: pos={}, end={}",
                i, node.pos, node.end
            );
            let kw_start = provider.skip_whitespace_forward(node.pos as usize);
            println!("  skip_whitespace_forward(pos={})={}", node.pos, kw_start);
            println!(
                "  text at kw_start: '{}'",
                &source[kw_start..kw_start.min(source.len()) + 10.min(source.len() - kw_start)]
            );
            if let Some(if_data) = arena.get_if_statement(node) {
                if let Some(then_node) = arena.get(if_data.then_statement) {
                    println!("  then: pos={}, end={}", then_node.pos, then_node.end);
                }
                if !if_data.else_statement.is_none()
                    && let Some(else_node) = arena.get(if_data.else_statement)
                {
                    println!(
                        "  else_statement: pos={}, end={}, kind={}",
                        else_node.pos, else_node.end, else_node.kind
                    );
                    if let Some(then_node) = arena.get(if_data.then_statement) {
                        let search_start = then_node.end as usize;
                        let search_end = else_node.end as usize;
                        let search_text = &source[search_start..search_end.min(source.len())];
                        println!(
                            "  search range: {}..{}, text: '{}'",
                            search_start, search_end, search_text
                        );
                        // Try to find "else" in this range
                        if let Some(else_pos) =
                            provider.find_keyword_in_range(search_start, search_end, "else")
                        {
                            println!("  FOUND 'else' at offset {}", else_pos);
                        } else {
                            println!("  DID NOT find 'else' in range");
                        }
                    }
                }
            }
        }
    }

    // Also test find_owning_if_statement
    println!(
        "\nfind_owning_if_statement(0)={:?}",
        provider.find_owning_if_statement(0)
    );
}
