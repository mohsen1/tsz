use std::fs;
use std::path::Path;

fn read_solver_source(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

#[test]
fn relation_cache_identity_has_dedicated_type_module() {
    let types_rs = read_solver_source("types.rs");
    let relation_cache_rs = read_solver_source("types/relation_cache.rs");

    assert!(
        types_rs.contains("mod relation_cache;")
            && types_rs.contains("pub use relation_cache::{")
            && relation_cache_rs.contains("pub enum CachedAnyMode")
            && relation_cache_rs.contains("pub enum RelationCacheKind")
            && relation_cache_rs.contains("pub struct RelationCacheConfig")
            && relation_cache_rs.contains("pub struct RelationCacheKey")
            && relation_cache_rs.contains("pub enum RelationCacheValue"),
        "solver relation-cache identity must be owned by types/relation_cache.rs"
    );

    for forbidden in [
        "pub enum CachedAnyMode",
        "pub enum RelationCacheKind",
        "pub struct RelationCacheConfig",
        "pub struct RelationCacheKey",
        "pub enum RelationCacheValue",
    ] {
        assert!(
            !types_rs.contains(forbidden),
            "types.rs must re-export `{forbidden}` instead of owning relation-cache identity"
        );
    }
}
