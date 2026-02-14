use super::*;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::ParserState;

fn transform_destructuring(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        return String::new();
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        return String::new();
    };

    // Get first statement (variable statement)
    let Some(&stmt_idx) = source_file.statements.nodes.first() else {
        return String::new();
    };
    let Some(stmt_node) = parser.arena.get(stmt_idx) else {
        return String::new();
    };

    if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
        return String::new();
    }

    let Some(var_data) = parser.arena.get_variable(stmt_node) else {
        return String::new();
    };

    // Get first declaration
    let Some(&decl_idx) = var_data.declarations.nodes.first() else {
        return String::new();
    };
    let Some(decl_node) = parser.arena.get(decl_idx) else {
        return String::new();
    };
    let Some(decl) = parser.arena.get_variable_declaration(decl_node) else {
        return String::new();
    };

    let mut transformer = ES5DestructuringTransformer::new(&parser.arena);
    let nodes = transformer.transform_destructuring_declaration(decl.name, decl.initializer);

    // Print the IR nodes
    let mut output = String::new();
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let mut printer = IRPrinter::with_arena(&parser.arena);
        printer.emit(node);
        output.push_str(printer.get_output());
        output.push(';');
    }

    output
}

#[test]
fn test_transformer_creation() {
    let arena = NodeArena::new();
    let transformer = ES5DestructuringTransformer::new(&arena);
    assert!(!transformer.is_destructuring_pattern(NodeIndex::NONE));
    assert!(!transformer.is_destructuring_assignment(NodeIndex::NONE));
}

#[test]
fn test_temp_var_generation() {
    let arena = NodeArena::new();
    let mut transformer = ES5DestructuringTransformer::new(&arena);

    // Test temp var name generation
    assert_eq!(transformer.next_temp_var(), "_a");
    assert_eq!(transformer.next_temp_var(), "_b");
    assert_eq!(transformer.next_temp_var(), "_c");

    // Reset and verify it starts over
    transformer.reset();
    assert_eq!(transformer.next_temp_var(), "_a");
}

#[test]
fn test_ir_node_generation() {
    use crate::transforms::ir::IRNode;

    // Test that we can build the expected IR structure
    let temp_var = "_a";
    let ir = IRNode::var_decl(temp_var, Some(IRNode::id("arr")));

    let mut printer = IRPrinter::new();
    printer.emit(&ir);
    let output = printer.get_output();

    assert!(output.contains("var _a"));
    assert!(output.contains("arr"));
}
