use super::*;
use crate::transforms::ir_printer::IRPrinter;

// =============================================================================
// Basic transformer tests
// =============================================================================

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

// =============================================================================
// Temp var counter behavior
// =============================================================================

#[test]
fn test_temp_var_wraps_alphabet() {
    let arena = NodeArena::new();
    let mut transformer = ES5DestructuringTransformer::new(&arena);

    // Generate 26 temp vars (a-z)
    for i in 0..26 {
        let expected_char = (b'a' + i as u8) as char;
        let expected = format!("_{expected_char}");
        assert_eq!(transformer.next_temp_var(), expected);
    }
}

#[test]
fn test_with_temp_counter() {
    let arena = NodeArena::new();
    let transformer = ES5DestructuringTransformer::new(&arena).with_temp_counter(5);
    assert_eq!(transformer.temp_var_counter(), 5);
}

#[test]
fn test_with_this_captured() {
    let arena = NodeArena::new();
    let transformer = ES5DestructuringTransformer::new(&arena).with_this_captured(true);
    // Just verify it compiles and doesn't panic
    assert!(!transformer.is_destructuring_pattern(NodeIndex::NONE));
}

// =============================================================================
// Pattern detection (with empty arena)
// =============================================================================

#[test]
fn test_is_destructuring_pattern_on_none_index() {
    let arena = NodeArena::new();
    let transformer = ES5DestructuringTransformer::new(&arena);
    assert!(!transformer.is_destructuring_pattern(NodeIndex::NONE));
}

#[test]
fn test_is_destructuring_assignment_on_none_index() {
    let arena = NodeArena::new();
    let transformer = ES5DestructuringTransformer::new(&arena);
    assert!(!transformer.is_destructuring_assignment(NodeIndex::NONE));
}

// =============================================================================
// IR structure tests
// =============================================================================

#[test]
fn test_ir_var_decl_with_index_access() {
    use crate::transforms::ir::IRNode;

    // Simulate: var a = _a[0]
    let ir = IRNode::var_decl(
        "a",
        Some(IRNode::ElementAccess {
            object: Box::new(IRNode::id("_a")),
            index: Box::new(IRNode::NumericLiteral("0".to_string())),
        }),
    );

    let mut printer = IRPrinter::new();
    printer.emit(&ir);
    let output = printer.get_output();

    assert!(
        output.contains("var a"),
        "Expected var declaration: {output}"
    );
    assert!(output.contains("_a[0]"), "Expected index access: {output}");
}

#[test]
fn test_ir_var_decl_with_property_access() {
    use crate::transforms::ir::IRNode;

    // Simulate: var x = _a.x
    let ir = IRNode::var_decl(
        "x",
        Some(IRNode::PropertyAccess {
            object: Box::new(IRNode::id("_a")),
            property: "x".to_string(),
        }),
    );

    let mut printer = IRPrinter::new();
    printer.emit(&ir);
    let output = printer.get_output();

    assert!(
        output.contains("var x"),
        "Expected var declaration: {output}"
    );
    assert!(
        output.contains("_a.x"),
        "Expected property access: {output}"
    );
}

#[test]
fn test_ir_void_0_for_undefined() {
    use crate::transforms::ir::IRNode;

    // void 0 is the canonical ES5 pattern for undefined
    let ir = IRNode::Undefined;

    let mut printer = IRPrinter::new();
    printer.emit(&ir);
    let output = printer.get_output();

    assert!(
        output.contains("void 0"),
        "Expected void 0 for undefined: {output}"
    );
}

#[test]
fn test_ir_conditional_for_default_value() {
    use crate::transforms::ir::IRNode;

    // Simulate: _a.x !== void 0 ? _a.x : 10
    let ir = IRNode::ConditionalExpr {
        condition: Box::new(IRNode::BinaryExpr {
            left: Box::new(IRNode::PropertyAccess {
                object: Box::new(IRNode::id("_a")),
                property: "x".to_string(),
            }),
            operator: "!==".to_string(),
            right: Box::new(IRNode::Undefined),
        }),
        when_true: Box::new(IRNode::PropertyAccess {
            object: Box::new(IRNode::id("_a")),
            property: "x".to_string(),
        }),
        when_false: Box::new(IRNode::NumericLiteral("10".to_string())),
    };

    let mut printer = IRPrinter::new();
    printer.emit(&ir);
    let output = printer.get_output();

    assert!(output.contains("!=="), "Expected !== operator: {output}");
    assert!(output.contains("void 0"), "Expected void 0: {output}");
    assert!(output.contains("10"), "Expected default value: {output}");
}

#[test]
fn test_ir_slice_for_rest_element() {
    use crate::transforms::ir::IRNode;

    // Simulate: _a.slice(1) for rest elements
    let ir = IRNode::CallExpr {
        callee: Box::new(IRNode::PropertyAccess {
            object: Box::new(IRNode::id("_a")),
            property: "slice".to_string(),
        }),
        arguments: vec![IRNode::NumericLiteral("1".to_string())],
    };

    let mut printer = IRPrinter::new();
    printer.emit(&ir);
    let output = printer.get_output();

    assert!(
        output.contains("_a.slice(1)"),
        "Expected slice call for rest element: {output}"
    );
}
