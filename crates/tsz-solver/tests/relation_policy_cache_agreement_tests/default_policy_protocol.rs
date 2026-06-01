//! Source ratchets for keeping default relation policies on typed constructors.

#[test]
fn default_relation_policy_paths_do_not_spell_packed_zero_flags() {
    let sources = [
        (
            "relation_queries.rs",
            include_str!("../../src/relations/relation_queries.rs"),
        ),
        ("caches/db.rs", include_str!("../../src/caches/db.rs")),
        (
            "caches/query_cache.rs",
            include_str!("../../src/caches/query_cache.rs"),
        ),
    ];

    for (name, source) in sources {
        assert!(
            !source.contains("RelationPolicy::from_flags(0)")
                && !source.contains("RelationPolicy::from_relation_flags(RelationFlags::empty())"),
            "{name} must use RelationPolicy::unflagged_compatibility() for default relation policy paths",
        );
    }
}
