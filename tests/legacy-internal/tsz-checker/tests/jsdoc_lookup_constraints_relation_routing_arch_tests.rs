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
        helper.contains("jsdoc_type_constraint_relation_outcome(type_arg, constraint)")
            && helper.contains(".related"),
        "JSDoc generic constraint validation should use the typed JSDoc relation outcome boundary"
    );
    assert!(
        !helper.contains("assign_relation_outcome(type_arg, constraint)"),
        "JSDoc generic constraint validation should not use the generic assignment request"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "JSDoc generic constraint validation should not regress to a raw boolean relation guard"
    );
}

#[test]
fn jsdoc_import_type_constraints_use_jsdoc_relation_request() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        fs::read_to_string(manifest_dir.join("src/jsdoc/diagnostics_import_type_constraints.rs"))
            .expect("read JSDoc import type constraint diagnostic source");

    assert!(
        source.contains("jsdoc_type_constraint_relation_outcome(type_arg, constraint)")
            && source.contains(".related"),
        "JSDoc import type constraints should use the typed JSDoc relation outcome boundary"
    );
    assert!(
        !source.contains("assign_relation_outcome(type_arg, constraint)"),
        "JSDoc import type constraints should not use the generic assignment request"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "JSDoc import type constraints should not regress to a raw boolean relation guard"
    );
}
