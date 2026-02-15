use super::*;
use crate::transforms::ir_printer::IRPrinter;

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
