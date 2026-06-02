use std::fs;
use std::path::Path;

/// Flow analysis relation helpers must stay on the boundary-owned
/// `RelationFlags` wrapper rather than reaching into solver internals.
#[test]
fn flow_analysis_uses_boundary_relation_flags_surface() {
    let source = fs::read_to_string("src/query_boundaries/flow_analysis.rs")
        .expect("failed to read query_boundaries/flow_analysis.rs");

    assert!(
        source.contains("assignability::RelationFlags"),
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

/// Checker relation helpers are the compatibility edge for packed `u16`
/// relation flags. Keep that edge explicit so new code does not treat the
/// packed protocol as an ordinary solver policy constructor.
#[test]
fn checker_boundaries_use_explicit_legacy_relation_policy_constructor() {
    for path in [
        "src/query_boundaries/assignability.rs",
        "src/query_boundaries/class.rs",
        "src/query_boundaries/flow_analysis.rs",
    ] {
        let source = fs::read_to_string(path).expect("failed to read checker query boundary");

        assert!(
            source.contains("relation_policy::from_checker_flags_u16"),
            "{path} must name the packed-flag compatibility edge explicitly",
        );
        assert!(
            !source.contains("RelationPolicy::from_flags"),
            "{path} must not use the ambiguous packed-flag constructor name",
        );
    }
}

/// `query_boundaries::relation_policy` is the only checker compatibility edge
/// that should name the solver packed-flag constructor directly.
#[test]
fn checker_query_boundaries_quarantine_raw_relation_policy_constructor() {
    for entry in fs::read_dir("src/query_boundaries").expect("failed to read query_boundaries") {
        let path = entry.expect("failed to read query boundary entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }

        if path == Path::new("src/query_boundaries/relation_policy.rs") {
            continue;
        }

        let source = fs::read_to_string(&path).expect("failed to read checker query boundary");
        assert!(
            !source.contains("RelationPolicy::from_flags"),
            "{} must route packed checker flags through relation_policy::from_checker_flags_u16",
            path.display(),
        );
    }
}
