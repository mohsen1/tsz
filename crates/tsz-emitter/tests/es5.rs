use super::*;
use crate::transforms::destructuring_es5::ES5DestructuringTransformer;

// =============================================================================
// Basic transformer smoke tests
// =============================================================================

#[test]
fn test_es5_class_transformer_basic() {
    // This would need actual AST nodes to test properly
    // For now, just verify the transformer compiles
    let arena = NodeArena::new();
    let mut transformer = ES5ClassTransformer::new(&arena);
    assert!(transformer.transform_class(NodeIndex::NONE).is_none());
}

#[test]
fn test_es5_async_transformer_basic() {
    let arena = NodeArena::new();
    let transformer = ES5AsyncTransformer::new(&arena);
    assert!(!transformer.body_contains_await(NodeIndex::NONE));
}

// =============================================================================
// ES5 Destructuring Transformer with parsed AST
// =============================================================================

#[test]
fn test_es5_destructuring_transformer_detects_array_pattern() {
    let source = "const [a, b] = arr;";
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let root_node = parser.arena.get(root).unwrap();
    let source_file = parser.arena.get_source_file(root_node).unwrap();
    let stmt_idx = source_file.statements.nodes[0];
    let stmt_node = parser.arena.get(stmt_idx).unwrap();

    // Get the variable statement
    let var_stmt = parser.arena.get_variable(stmt_node).unwrap();

    // Get the declaration list
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list_node = parser.arena.get(decl_list_idx).unwrap();
    let decl_list = parser.arena.get_variable(decl_list_node).unwrap();

    // Get the variable declaration
    let decl_idx = decl_list.declarations.nodes[0];
    let decl_node = parser.arena.get(decl_idx).unwrap();
    let decl = parser.arena.get_variable_declaration(decl_node).unwrap();

    let transformer = ES5DestructuringTransformer::new(&parser.arena);

    // The name should be an array binding pattern
    assert!(
        transformer.is_destructuring_pattern(decl.name),
        "Expected array destructuring to be detected as pattern"
    );
}

#[test]
fn test_es5_destructuring_transformer_detects_object_pattern() {
    let source = "const {x, y} = obj;";
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let root_node = parser.arena.get(root).unwrap();
    let source_file = parser.arena.get_source_file(root_node).unwrap();
    let stmt_idx = source_file.statements.nodes[0];
    let stmt_node = parser.arena.get(stmt_idx).unwrap();

    let var_stmt = parser.arena.get_variable(stmt_node).unwrap();
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list_node = parser.arena.get(decl_list_idx).unwrap();
    let decl_list = parser.arena.get_variable(decl_list_node).unwrap();
    let decl_idx = decl_list.declarations.nodes[0];
    let decl_node = parser.arena.get(decl_idx).unwrap();
    let decl = parser.arena.get_variable_declaration(decl_node).unwrap();

    let transformer = ES5DestructuringTransformer::new(&parser.arena);

    assert!(
        transformer.is_destructuring_pattern(decl.name),
        "Expected object destructuring to be detected as pattern"
    );
}

// =============================================================================
// ES5 Class Transformer with parsed AST
// =============================================================================

#[test]
fn test_es5_class_transformer_none_for_empty_class() {
    let source = "class Empty {}";
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let root_node = parser.arena.get(root).unwrap();
    let source_file = parser.arena.get_source_file(root_node).unwrap();
    let class_idx = source_file.statements.nodes[0];

    let mut transformer = ES5ClassTransformer::new(&parser.arena);
    let result = transformer.transform_class(class_idx);

    // Empty class should still produce output (constructor + return)
    assert!(
        result.is_some(),
        "Even empty class should produce ES5 output"
    );
}

#[test]
fn test_es5_class_transformer_skips_declare_class() {
    let source = "declare class Ambient { foo(): void; }";
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let root_node = parser.arena.get(root).unwrap();
    let source_file = parser.arena.get_source_file(root_node).unwrap();
    let class_idx = source_file.statements.nodes[0];

    let mut transformer = ES5ClassTransformer::new(&parser.arena);
    let result = transformer.transform_class(class_idx);

    // Declare/ambient classes should not produce ES5 output
    assert!(result.is_none(), "Declare class should return None");
}

// =============================================================================
// ES5 Async Transformer detection
// =============================================================================

#[test]
fn test_es5_async_transformer_detects_await_in_body() {
    let source = "async function f() { await Promise.resolve(); }";
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let root_node = parser.arena.get(root).unwrap();
    let source_file = parser.arena.get_source_file(root_node).unwrap();
    let func_idx = source_file.statements.nodes[0];
    let func_node = parser.arena.get(func_idx).unwrap();
    let func_data = parser.arena.get_function(func_node).unwrap();

    let transformer = ES5AsyncTransformer::new(&parser.arena);
    let has_await = transformer.body_contains_await(func_data.body);

    assert!(
        has_await,
        "Expected await to be detected in async function body"
    );
}

/// Regression test: the `can_inline_wrapper` early-return path in
/// `emit_async_function_es5_body` must call `pop_temp_scope()` to restore
/// the temp-variable state pushed by `emit_function_parameters_es5`.
/// Without it, the printer leaks state across functions, leaving the
/// scope stack non-empty after a top-level async function has been emitted.
/// Devin review: <https://github.com/mohsen1/tsz/pull/2305#discussion_r3176690840>
#[test]
fn test_es5_async_inline_wrapper_pops_temp_scope() {
    use crate::emitter::{Printer, PrinterOptions};
    use tsz_common::ScriptTarget;
    let source = "async function f() { return 1; }\n";
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    assert!(
        printer.temp_scope_stack.is_empty(),
        "Top-level emit must leave the temp-scope stack empty; got len={}",
        printer.temp_scope_stack.len()
    );
}

#[test]
fn test_es5_async_transformer_no_await_in_sync_body() {
    let source = "function f() { return 42; }";
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let root_node = parser.arena.get(root).unwrap();
    let source_file = parser.arena.get_source_file(root_node).unwrap();
    let func_idx = source_file.statements.nodes[0];
    let func_node = parser.arena.get(func_idx).unwrap();
    let func_data = parser.arena.get_function(func_node).unwrap();

    let transformer = ES5AsyncTransformer::new(&parser.arena);
    let has_await = transformer.body_contains_await(func_data.body);

    assert!(!has_await, "Expected no await in synchronous function body");
}
