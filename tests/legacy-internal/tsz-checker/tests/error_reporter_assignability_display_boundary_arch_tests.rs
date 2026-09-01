use std::fs;
use std::path::Path;

/// Display-only assignability/alias `error_reporter` modules must obtain their
/// type-shape facts through `query_boundaries::diagnostics`, never the catch-all
/// `query_boundaries::common` boundary (issue #12947, slice 1). Routing render
/// policy through a single domain boundary keeps diagnostic shape queries
/// distinguishable from generic type queries and prevents reintroducing
/// formatted-string predicates against `common`.
#[test]
fn error_reporter_assignability_display_avoids_common_boundary() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in [
        "src/error_reporter/assignability.rs",
        "src/error_reporter/assignability_helpers.rs",
        "src/error_reporter/assignability_alias_display.rs",
        "src/error_reporter/assignability_keyof_alias_display.rs",
        "src/error_reporter/assignability_enum_display.rs",
        "src/error_reporter/conditional_alias_display.rs",
        "src/error_reporter/core_alias_display.rs",
        "src/error_reporter/literal_alias_rewrites.rs",
    ] {
        let source = fs::read_to_string(manifest.join(file))
            .unwrap_or_else(|err| panic!("failed to read {file}: {err}"));
        assert!(
            !source.contains("query_boundaries::common"),
            "{file} must route display shape queries through query_boundaries::diagnostics, \
             not query_boundaries::common"
        );
    }
}

/// The display-shape predicates routed off `common` for slice 1 must remain
/// reachable through the `diagnostics` boundary, so the migrated modules keep a
/// single import surface for diagnostic render policy.
#[test]
fn diagnostics_boundary_reexports_routed_display_helpers() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let diagnostics = fs::read_to_string(manifest.join("src/query_boundaries/diagnostics.rs"))
        .expect("failed to read query_boundaries/diagnostics.rs");

    for helper in [
        "is_conditional_type",
        "is_generic_application",
        "is_literal_type",
        "is_mapped_type",
        "is_type_parameter",
        "is_type_parameter_like",
        "is_type_query_type",
        "is_union_type",
        "widen_type",
        "IndexKind",
        // The resolver itself stays owned by `query_boundaries::index_signature`
        // (see `index_signature_boundary_scans`); `diagnostics` routes the fact,
        // not the resolver, so display call sites never construct one.
        "has_index_signature",
    ] {
        assert!(
            diagnostics.contains(helper),
            "{helper} must remain available through query_boundaries::diagnostics for \
             error_reporter display routing"
        );
    }
}
