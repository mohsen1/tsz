use std::fs;
use std::path::Path;

/// Ratchet: indexed-access assignability normalization must ask its low-level
/// type-shape questions through the assignability boundary, not the catch-all
/// `query_boundaries::common` module. Normalization runs inside the TS2322 /
/// TS2345 relation pipeline, so its shape probes are relation-adjacent and
/// belong behind `query_boundaries::assignability`.
#[test]
fn assignability_index_access_normalization_avoids_common_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/index_access_normalization.rs"),
    )
    .expect("failed to read index_access_normalization.rs");

    assert!(
        !source.contains("query_boundaries::common::"),
        "index-access assignability normalization must use assignability boundary helpers, \
         not query_boundaries::common"
    );
    assert!(
        source.contains("query_boundaries::assignability::is_index_access_for_assignability"),
        "normalization must detect indexed access through the named assignability boundary helper"
    );
    assert!(
        source.contains("query_boundaries::assignability::union_members_for_assignability"),
        "normalization must peel union members through the named assignability boundary helper"
    );
}
