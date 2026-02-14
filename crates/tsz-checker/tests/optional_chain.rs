use super::*;
use tsz_parser::parser::node::NodeArena;

#[test]
fn test_optional_chain_info_creation() {
    let arena = NodeArena::new();
    let info = analyze_optional_chain(&arena, NodeIndex::NONE);
    assert!(!info.is_optional);
    assert!(!info.is_immediate_optional);
}

#[test]
fn test_is_optional_chain_empty() {
    let arena = NodeArena::new();
    assert!(!is_optional_chain(&arena, NodeIndex::NONE));
}
