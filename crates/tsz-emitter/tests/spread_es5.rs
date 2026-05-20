use super::*;
use tsz_parser::parser::node::NodeArena;

#[test]
fn test_transformer_creation() {
    let arena = NodeArena::new();
    let transformer = ES5SpreadTransformer::new(&arena);
    assert!(!transformer.array_contains_spread(NodeIndex::NONE));
    assert!(!transformer.call_contains_spread(NodeIndex::NONE));
    assert!(!transformer.object_contains_spread(NodeIndex::NONE));
}

#[test]
fn test_transformer_with_options() {
    let arena = NodeArena::new();
    let options = SpreadTransformOptions {
        use_spread_helper: true,
        use_assign_helper: true,
    };
    let transformer = ES5SpreadTransformer::with_options(&arena, options);
    assert!(transformer.options.use_spread_helper);
    assert!(transformer.options.use_assign_helper);
}
