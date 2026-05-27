use std::fs;
use std::path::Path;

#[test]
fn type_param_defaults_use_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path =
        Path::new(manifest_dir).join("src/state/type_analysis/type_param_defaults.rs");
    let source = fs::read_to_string(source_path).expect("read type_param_defaults.rs");

    let function_start = source
        .find("fn validate_type_parameter_defaults_against_constraints")
        .expect("find type parameter default validation function");
    let diagnostic_start = source[function_start..]
        .find("self.error_at_node_msg(")
        .expect("find default constraint diagnostic emission");
    let branch = &source[function_start..function_start + diagnostic_start];

    assert!(
        branch.contains(".assign_relation_outcome(default_type, constraint_type)")
            && branch.contains(".assign_relation_outcome(evaluated_default, evaluated_constraint)")
            && branch.contains(".assign_relation_outcome(evaluated_default, constraint_type)"),
        "type-parameter default relation decisions should use relation outcomes"
    );
    assert!(
        !branch.contains("diagnostic_relation_boolean_guard"),
        "type-parameter default validation should not fall back to raw boolean relation guards"
    );
}
