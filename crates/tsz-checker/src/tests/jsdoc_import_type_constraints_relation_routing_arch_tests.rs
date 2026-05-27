use std::fs;
use std::path::PathBuf;

#[test]
fn jsdoc_import_type_constraints_use_relation_outcome_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        fs::read_to_string(manifest_dir.join("src/jsdoc/diagnostics_import_type_constraints.rs"))
            .expect("read JSDoc import type constraint diagnostics source");

    let helper = source
        .split("pub(super) fn report_jsdoc_import_type_constraint_error")
        .nth(1)
        .expect("find JSDoc import type constraint diagnostic helper")
        .split("for (name, previous) in scope_updates")
        .next()
        .expect("slice helper body before scope restoration");

    assert!(
        helper.contains("assign_relation_outcome(type_arg, constraint)")
            && helper.contains(".related"),
        "JSDoc import type constraint diagnostics should use the shared relation outcome boundary"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "JSDoc import type constraint diagnostics should not regress to a raw boolean relation guard"
    );
}
