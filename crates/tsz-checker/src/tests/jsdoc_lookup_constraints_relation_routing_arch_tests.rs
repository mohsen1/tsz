use std::fs;
use std::path::PathBuf;

#[test]
fn jsdoc_lookup_constraints_use_relation_outcome_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(manifest_dir.join("src/jsdoc/lookup.rs"))
        .expect("read JSDoc lookup source");

    let helper = source
        .split("fn validate_jsdoc_generic_constraints_at_node")
        .nth(1)
        .expect("find JSDoc generic constraint validation helper")
        .split("/// Resolve a direct leading JSDoc")
        .next()
        .expect("slice helper body before the next JSDoc lookup helper");

    assert!(
        helper.contains("assign_relation_outcome(type_arg, constraint)")
            && helper.contains(".related"),
        "JSDoc generic constraint validation should use the shared relation outcome boundary"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "JSDoc generic constraint validation should not regress to a raw boolean relation guard"
    );
}
