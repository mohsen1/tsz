use std::fs;

/// Flow analysis relation helpers must stay on the boundary-owned
/// `RelationFlags` wrapper rather than reaching into solver internals.
#[test]
fn flow_analysis_uses_boundary_relation_flags_surface() {
    let source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read query_boundaries/flow_analysis.rs");

    assert!(
        source.contains("use super::assignability::RelationFlags;"),
        "flow_analysis relation helpers must import boundary-owned RelationFlags"
    );

    assert!(
        source.contains("RelationFlags::STRICT_NULL_CHECKS"),
        "flow_analysis relation helpers must use RelationFlags when encoding strict-null policy"
    );

    assert!(
        !source.contains("RelationCacheKey::FLAG_STRICT_NULL_CHECKS"),
        "flow_analysis relation helpers must not reach directly into RelationCacheKey bits"
    );
}
