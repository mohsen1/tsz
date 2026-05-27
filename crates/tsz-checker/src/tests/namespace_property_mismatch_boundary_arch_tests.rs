use std::fs;
use std::path::Path;

#[test]
fn namespace_property_mismatch_uses_narrow_boundaries() {
    let source_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assignability/assignability_relation.rs");
    let source = fs::read_to_string(&source_path).expect("read assignability relation source");

    let function_start = source
        .find("fn namespace_source_has_matching_property_mismatch")
        .expect("find namespace property mismatch helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    pub(crate) fn execute_relation_request")
        .expect("find next helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("query_boundaries::common::"),
        "namespace property mismatch lookup should use narrow query-boundary helpers"
    );
    assert!(
        function.contains("get_union_members"),
        "namespace target union peeling should route through the assignability boundary"
    );
    assert!(
        function.contains("get_lazy_def_id"),
        "namespace target lazy lookup should route through the type-resolution boundary"
    );
    assert!(
        function.matches("object_shape_for_type").count() >= 2,
        "namespace object-shape fallback order should route through the assignability boundary"
    );
}
