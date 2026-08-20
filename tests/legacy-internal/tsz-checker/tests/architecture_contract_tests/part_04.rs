//! Contiguous test shard split out of the parent module to satisfy the
//! source-file line cap.

use super::*;

/// Relation request property policy must be executable boundary input, not
/// future-facing documentation that callers can ignore.
#[test]
fn test_relation_request_property_policy_is_executed() {
    let request_source = fs::read_to_string("src/query_boundaries/relation_request.rs")
        .expect("failed to read query_boundaries/relation_request.rs");
    let boundary_source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        !request_source.contains("Currently advisory"),
        "RelationRequest property-policy fields must not be documented as advisory-only"
    );
    assert!(
        request_source.contains("fn requires_property_classification(&self) -> bool"),
        "RelationRequest must expose an executable policy query for property classification"
    );
    assert!(
        request_source.contains("fn solver_relation_policy("),
        "RelationRequest must expose executable solver relation kind/flag policy"
    );

    let execute_body = boundary_source
        .split("pub(crate) fn execute_relation")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn suppress_excess_property_failure_if_needed")
                .next()
        })
        .expect("failed to locate execute_relation body");
    assert!(
        execute_body.contains("request.requires_property_classification()"),
        "execute_relation must use request property policy before classifying object properties"
    );
    assert!(
        execute_body.contains("request.solver_relation_policy("),
        "execute_relation must ask RelationRequest for solver kind/flags instead of open-coding request policy"
    );
}

/// Display-only diagnostic helpers should not live in the catch-all
/// `query_boundaries::common` module.
#[test]
fn test_display_diagnostic_helpers_have_domain_boundary() {
    let common_source = fs::read_to_string("src/query_boundaries/common.rs")
        .expect("failed to read query_boundaries/common.rs");
    let diagnostic_source = fs::read_to_string("src/query_boundaries/diagnostics.rs")
        .expect("failed to read query_boundaries/diagnostics.rs");

    for helper in [
        "indexed_access_alias_body",
        "is_unresolved_for_display",
        "type_may_display_iterator_protocol",
        "function_signature_has_typeof",
        "get_object_symbol",
    ] {
        assert!(
            !common_source.contains(&format!("fn {helper}(")),
            "{helper} belongs in query_boundaries::diagnostics, not common.rs"
        );
        assert!(
            diagnostic_source.contains(&format!("fn {helper}(")),
            "{helper} must remain available through query_boundaries::diagnostics"
        );
    }
}

/// Checker compiler-option packing must stay on the boundary-owned
/// `RelationFlags` wrapper rather than reaching into solver internals.
#[test]
fn test_pack_relation_flags_uses_boundary_relation_flags_surface() {
    let source = fs::read_to_string("src/context/compiler_options.rs")
        .expect("failed to read context/compiler_options.rs");

    assert!(
        source.contains("use crate::query_boundaries::assignability::RelationFlags;"),
        "pack_relation_flags must import boundary-owned RelationFlags"
    );

    for flag in [
        "RelationFlags::STRICT_NULL_CHECKS",
        "RelationFlags::STRICT_FUNCTION_TYPES",
        "RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES",
        "RelationFlags::NO_UNCHECKED_INDEXED_ACCESS",
        "RelationFlags::ALLOW_BIVARIANT_REST",
    ] {
        assert!(
            source.contains(flag),
            "pack_relation_flags must use `{flag}` when encoding checker policy"
        );
    }

    assert!(
        !source.contains("RelationCacheKey::FLAG_STRICT_NULL_CHECKS"),
        "pack_relation_flags must not reach directly into RelationCacheKey bits"
    );
}
