use super::*;

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
