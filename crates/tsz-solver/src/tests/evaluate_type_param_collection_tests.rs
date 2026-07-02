//! Guardrails for type-parameter extraction used by conditional evaluation.
//!
//! Structural rule: when permissive conditional instantiation asks which type
//! parameters occur in a check/extends type, the evaluator must walk each
//! `TypeId` at most once — project-wide, not merely per query. Recursive
//! mapped/conditional application graphs revisit the same structural nodes on
//! every unwrap step (each step mints a fresh root over an already-walked
//! interior), so the collector memoizes every node's reachable-parameter list
//! on the shared interner and prunes provably-empty subtrees through the
//! cached `contains_extractable_type_params_db` reachability gate (#13508).

use std::fs;
use std::path::Path;

#[test]
fn evaluator_type_param_collection_memoizes_per_type_id() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/evaluation/evaluate/support.rs");
    let source = fs::read_to_string(path).expect("failed to read evaluation/evaluate/support.rs");
    let compact: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact.contains("self.interner.extract_type_params_memo(type_id)"),
        "type-param extraction should consult the shared per-TypeId memo at every node"
    );
    assert!(
        compact.contains(".set_extract_type_params_memo(type_id,"),
        "type-param extraction should persist every visited node's list, so a \
         fresh root over an already-walked interior is O(new nodes)"
    );
    assert!(
        compact.contains("contains_extractable_type_params_db(self.interner,type_id)"),
        "type-param extraction should prune provably-empty subtrees through the \
         cached reachability gate"
    );
}
