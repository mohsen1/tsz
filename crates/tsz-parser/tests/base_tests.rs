//! Unit tests for `parser/base.rs`: pure-additive coverage of the foundational
//! `NodeIndex`, `NodeList`, and `TextRange` types used throughout the thin
//! pipeline.
//!
//! The only existing base.rs coverage was three lines in `tests/tests.rs`
//! that spot-checked `NodeIndex::is_some`/`is_none`. This file exercises every
//! public method, the `NONE` sentinel, default values, and serde round-trips.

use super::*;

// =====================================================================
// NodeIndex sentinel and constructors
// =====================================================================

#[test]
fn node_index_none_sentinel_is_u32_max() {
    // The NONE sentinel must be exactly u32::MAX so callers can compare
    // against it via `==` or use `is_none`.
    assert_eq!(NodeIndex::NONE.0, u32::MAX);
}

#[test]
fn node_index_default_equals_zero() {
    // Default is `NodeIndex(0)`, NOT `NodeIndex::NONE`. Callers that want a
    // missing-index sentinel must use `NodeIndex::NONE` explicitly.
    let default = NodeIndex::default();
    assert_eq!(default.0, 0);
    assert!(default.is_some());
    assert!(!default.is_none());
}

#[test]
fn node_index_zero_is_some() {
    // `NodeIndex(0)` is a valid index (typically the SourceFile root) — must
    // not be confused with NONE.
    let idx = NodeIndex(0);
    assert!(idx.is_some());
    assert!(!idx.is_none());
}

#[test]
fn node_index_max_minus_one_is_some() {
    // The largest non-NONE value still reports as some.
    let idx = NodeIndex(u32::MAX - 1);
    assert!(idx.is_some());
    assert!(!idx.is_none());
}

#[test]
fn node_index_none_reports_none() {
    let none = NodeIndex::NONE;
    assert!(none.is_none());
    assert!(!none.is_some());
}

// =====================================================================
// NodeIndex::into_option
// =====================================================================

#[test]
fn node_index_into_option_none_returns_none() {
    assert_eq!(NodeIndex::NONE.into_option(), None);
}

#[test]
fn node_index_into_option_some_returns_some_self() {
    let idx = NodeIndex(7);
    assert_eq!(idx.into_option(), Some(idx));
}

#[test]
fn node_index_into_option_zero_returns_some_zero() {
    // Zero is a real index, not NONE.
    let idx = NodeIndex(0);
    assert_eq!(idx.into_option(), Some(NodeIndex(0)));
}

#[test]
fn node_index_into_option_max_minus_one_returns_some() {
    let idx = NodeIndex(u32::MAX - 1);
    assert_eq!(idx.into_option(), Some(idx));
}

// =====================================================================
// NodeIndex equality, copy, hashing
// =====================================================================

#[test]
#[allow(clippy::clone_on_copy)] // Intentional: verify Copy + Clone both compile
fn node_index_is_copy_and_clone() {
    let a = NodeIndex(42);
    let b = a; // copy
    let c = a.clone(); // explicit clone
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn node_index_equal_for_same_value() {
    assert_eq!(NodeIndex(5), NodeIndex(5));
    assert_ne!(NodeIndex(5), NodeIndex(6));
    assert_ne!(NodeIndex(0), NodeIndex::NONE);
}

#[test]
fn node_index_can_be_hash_map_key() {
    use std::collections::HashMap;
    let mut map: HashMap<NodeIndex, &str> = HashMap::new();
    map.insert(NodeIndex(1), "one");
    map.insert(NodeIndex(2), "two");
    map.insert(NodeIndex::NONE, "none");
    assert_eq!(map.get(&NodeIndex(1)), Some(&"one"));
    assert_eq!(map.get(&NodeIndex(2)), Some(&"two"));
    assert_eq!(map.get(&NodeIndex::NONE), Some(&"none"));
}

// =====================================================================
// NodeIndex serde round-trip
// =====================================================================

#[test]
fn node_index_serde_round_trip() {
    let original = NodeIndex(123);
    let json = serde_json::to_string(&original).expect("serialize NodeIndex");
    let parsed: NodeIndex = serde_json::from_str(&json).expect("deserialize NodeIndex");
    assert_eq!(parsed, original);
}

#[test]
fn node_index_none_serde_round_trip() {
    let json = serde_json::to_string(&NodeIndex::NONE).expect("serialize NONE");
    let parsed: NodeIndex = serde_json::from_str(&json).expect("deserialize NONE");
    assert_eq!(parsed, NodeIndex::NONE);
    assert!(parsed.is_none());
}

// =====================================================================
// NodeList::new and Default
// =====================================================================

#[test]
fn node_list_new_is_empty() {
    let list = NodeList::new();
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
    assert_eq!(list.pos, 0);
    assert_eq!(list.end, 0);
    assert!(!list.has_trailing_comma);
    assert!(list.nodes.is_empty());
}

#[test]
fn node_list_default_matches_new() {
    let default_list = NodeList::default();
    let new_list = NodeList::new();
    assert_eq!(default_list.len(), new_list.len());
    assert_eq!(default_list.pos, new_list.pos);
    assert_eq!(default_list.end, new_list.end);
    assert_eq!(default_list.has_trailing_comma, new_list.has_trailing_comma);
    assert_eq!(default_list.nodes.len(), new_list.nodes.len());
}

// =====================================================================
// NodeList::with_capacity
// =====================================================================

#[test]
fn node_list_with_capacity_is_empty_but_reserved() {
    let list = NodeList::with_capacity(8);
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
    assert!(list.nodes.capacity() >= 8);
}

#[test]
fn node_list_with_capacity_zero_is_empty() {
    let list = NodeList::with_capacity(0);
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
}

// =====================================================================
// NodeList::push, len, is_empty
// =====================================================================

#[test]
fn node_list_push_increases_len() {
    let mut list = NodeList::new();
    assert_eq!(list.len(), 0);
    list.push(NodeIndex(1));
    assert_eq!(list.len(), 1);
    assert!(!list.is_empty());
    list.push(NodeIndex(2));
    list.push(NodeIndex(3));
    assert_eq!(list.len(), 3);
}

#[test]
fn node_list_push_preserves_order() {
    let mut list = NodeList::new();
    list.push(NodeIndex(10));
    list.push(NodeIndex(20));
    list.push(NodeIndex(30));
    assert_eq!(list.nodes[0], NodeIndex(10));
    assert_eq!(list.nodes[1], NodeIndex(20));
    assert_eq!(list.nodes[2], NodeIndex(30));
}

#[test]
fn node_list_can_push_none_sentinel() {
    // `NodeIndex::NONE` is a valid value to store inside a NodeList; the
    // base type does not filter it.
    let mut list = NodeList::new();
    list.push(NodeIndex::NONE);
    assert_eq!(list.len(), 1);
    assert!(list.nodes[0].is_none());
}

// =====================================================================
// NodeList serde round-trip
// =====================================================================

#[test]
fn node_list_serde_round_trip() {
    let mut original = NodeList::new();
    original.push(NodeIndex(1));
    original.push(NodeIndex(42));
    original.pos = 5;
    original.end = 80;
    original.has_trailing_comma = true;

    let json = serde_json::to_string(&original).expect("serialize NodeList");
    let parsed: NodeList = serde_json::from_str(&json).expect("deserialize NodeList");
    assert_eq!(parsed.len(), original.len());
    assert_eq!(parsed.nodes, original.nodes);
    assert_eq!(parsed.pos, original.pos);
    assert_eq!(parsed.end, original.end);
    assert_eq!(parsed.has_trailing_comma, original.has_trailing_comma);
}

#[test]
fn node_list_empty_serde_round_trip() {
    let original = NodeList::new();
    let json = serde_json::to_string(&original).expect("serialize empty NodeList");
    let parsed: NodeList = serde_json::from_str(&json).expect("deserialize empty NodeList");
    assert!(parsed.is_empty());
    assert_eq!(parsed.pos, 0);
    assert_eq!(parsed.end, 0);
    assert!(!parsed.has_trailing_comma);
}

// =====================================================================
// TextRange constructor and defaults
// =====================================================================

#[test]
fn text_range_default_is_zero() {
    let range = TextRange::default();
    assert_eq!(range.pos, 0);
    assert_eq!(range.end, 0);
}

#[test]
fn text_range_new_stores_fields() {
    let range = TextRange::new(10, 25);
    assert_eq!(range.pos, 10);
    assert_eq!(range.end, 25);
}

#[test]
fn text_range_new_zero_is_empty_range() {
    let range = TextRange::new(0, 0);
    assert_eq!(range.pos, 0);
    assert_eq!(range.end, 0);
}

#[test]
fn text_range_new_pos_can_equal_end() {
    // Empty (zero-length) ranges with non-zero position are legal — a parser
    // recovery diagnostic may emit a span like `pos=5 end=5` for a missing
    // token at offset 5.
    let range = TextRange::new(5, 5);
    assert_eq!(range.pos, 5);
    assert_eq!(range.end, 5);
}

#[test]
#[allow(clippy::clone_on_copy)] // Intentional: verify Copy + Clone both compile
fn text_range_is_copy_and_clone() {
    let a = TextRange::new(1, 4);
    let b = a; // copy
    let c = a.clone(); // explicit clone
    assert_eq!(a.pos, b.pos);
    assert_eq!(a.end, b.end);
    assert_eq!(a.pos, c.pos);
    assert_eq!(a.end, c.end);
}

// =====================================================================
// TextRange serde round-trip (custom Deserialize)
// =====================================================================

#[test]
fn text_range_serde_round_trip() {
    let original = TextRange::new(7, 42);
    let json = serde_json::to_string(&original).expect("serialize TextRange");
    let parsed: TextRange = serde_json::from_str(&json).expect("deserialize TextRange");
    assert_eq!(parsed.pos, original.pos);
    assert_eq!(parsed.end, original.end);
}

#[test]
fn text_range_serde_default_round_trip() {
    let original = TextRange::default();
    let json = serde_json::to_string(&original).expect("serialize default TextRange");
    let parsed: TextRange = serde_json::from_str(&json).expect("deserialize default TextRange");
    assert_eq!(parsed.pos, 0);
    assert_eq!(parsed.end, 0);
}

#[test]
fn text_range_deserializes_from_explicit_json() {
    // The custom Deserialize impl reads {pos, end} from a JSON object.
    let json = r#"{"pos":100,"end":200}"#;
    let parsed: TextRange = serde_json::from_str(json).expect("explicit JSON parses");
    assert_eq!(parsed.pos, 100);
    assert_eq!(parsed.end, 200);
}
