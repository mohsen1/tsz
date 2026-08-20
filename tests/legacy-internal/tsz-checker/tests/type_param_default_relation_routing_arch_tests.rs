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
    let compact_branch: String = branch.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_branch
            .contains(".type_parameter_default_relation_outcome(default_type,constraint_type")
            && compact_branch.contains(
                ".type_parameter_default_relation_outcome(evaluated_default,evaluated_constraint"
            )
            && compact_branch.contains(
                ".type_parameter_default_relation_outcome(evaluated_default,constraint_type"
            ),
        "type-parameter default relation decisions should use role-specific relation outcomes"
    );
    assert!(
        !branch.contains("diagnostic_relation_boolean_guard"),
        "type-parameter default validation should not fall back to raw boolean relation guards"
    );
    assert!(
        !compact_branch.contains(".assign_relation_outcome(default_type,constraint_type)")
            && !compact_branch
                .contains(".assign_relation_outcome(evaluated_default,evaluated_constraint")
            && !compact_branch
                .contains(".assign_relation_outcome(evaluated_default,constraint_type"),
        "type-parameter default validation should not use generic assign relation outcomes"
    );
}

#[test]
fn type_parameter_default_relation_outcome_uses_default_request() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn type_parameter_default_relation_outcome(")
            && source.contains("RelationRequest::type_parameter_default("),
        "type-parameter default diagnostics should have a request-shaped RelationKind::TypeParameterDefault helper"
    );
}

#[test]
fn clean_type_parameter_default_validation_is_cached_per_checker_file() {
    let source = fs::read_to_string("src/state/type_analysis/type_param_defaults.rs")
        .expect("failed to read type_param_defaults.rs");
    let caches = fs::read_to_string("src/context/caches.rs").expect("failed to read caches.rs");
    let reset =
        fs::read_to_string("src/context/file_session_reset.rs").expect("failed to read reset.rs");

    assert!(
        source.contains("type_param_default_constraint")
            && source.contains("let diagnostics_before = self.ctx.diagnostics.len();")
            && source.contains("diagnostics.len() == diagnostics_before"),
        "successful default/constraint validations should be cached only after a clean diagnostic pass"
    );
    assert!(
        caches.contains("pub type_param_default_constraint: FxHashSet<(u32, TypeId, TypeId)>"),
        "default/constraint validation cache should be keyed by declaration and resolved types"
    );
    assert!(
        reset.contains(".type_param_default_constraint") && reset.contains(".clear()"),
        "default/constraint validation cache should reset between checker file sessions"
    );
}
