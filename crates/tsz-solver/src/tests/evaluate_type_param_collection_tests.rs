//! Guardrails for type-parameter extraction used by conditional evaluation.
//!
//! Structural rule: when permissive conditional instantiation asks which type
//! parameters occur in a check/extends type, the evaluator must walk the type
//! graph once per `TypeId`. Recursive mapped/conditional application graphs may
//! revisit the same structural node while proving assignability; parameter-name
//! deduplication alone does not terminate that traversal.

use std::fs;
use std::path::Path;

#[test]
fn evaluator_type_param_collection_tracks_visited_type_ids() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/evaluation/evaluate/support.rs");
    let source = fs::read_to_string(path).expect("failed to read evaluation/evaluate/support.rs");
    let compact: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact.contains("visited:&mutFxHashSet<TypeId>"),
        "type-param extraction should carry a visited TypeId set"
    );
    assert!(
        compact.contains("!visited.insert(type_id)"),
        "type-param extraction should return when a TypeId has already been visited"
    );
}
