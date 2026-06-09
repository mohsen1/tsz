use std::fs;
use std::path::Path;

#[test]
fn generic_constraint_validation_no_weak_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/checkers/generic_checker/constraint_validation.rs"),
    )
    .expect("failed to read constraint_validation.rs");

    assert_eq!(
        source
            .matches("type_arg_constraint_no_weak_relation_outcome(")
            .count(),
        3,
        "generic constraint validation should route no-weak relation probes through the named type-argument constraint fallback"
    );
    assert!(
        !source.contains("self.no_weak_relation_outcome(")
            && !source.contains("diagnostic_relation_boolean_guard_no_weak_checks("),
        "generic constraint validation should not use the raw no-weak relation helpers"
    );
}

#[test]
fn generic_constraint_validation_regular_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/checkers/generic_checker/constraint_validation.rs"),
    )
    .expect("failed to read constraint_validation.rs");

    assert!(
        source
            .matches("type_arg_constraint_relation_outcome(")
            .count()
            >= 9,
        "generic constraint validation should route plain type-argument probes through a named RelationRequest"
    );
    assert!(
        source
            .matches("conditional_constraint_component_relation_outcome(")
            .count()
            >= 4,
        "generic constraint validation should route conditional component probes through a named RelationRequest"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "generic constraint validation should not use the raw boolean relation guard"
    );
}

#[test]
fn successful_type_arg_constraint_relations_are_file_local_cached() {
    let relation_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assignability/relation_outcome_helpers.rs"),
    )
    .expect("failed to read relation_outcome_helpers.rs");
    let caches_source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/context/caches.rs"))
            .expect("failed to read caches.rs");
    let reset_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/context/file_session_reset.rs"),
    )
    .expect("failed to read file_session_reset.rs");

    assert!(
        caches_source.contains("type_arg_constraint_relation_successes")
            && relation_source.contains("type_arg_constraint_relation_successes")
            && relation_source.contains("pack_relation_flags")
            && relation_source.contains("sound_mode")
            && relation_source.contains("outcome.related")
            && reset_source.contains("type_arg_constraint_relation_successes")
            && reset_source.contains(".clear()"),
        "successful type-argument constraint relation probes should be cached by \
         prepared source/target plus relation mode, while failures keep the real \
         diagnostic relation path"
    );
}

#[test]
fn generic_constraint_validation_infer_result_checks_use_named_request() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/checkers/generic_checker/constraint_validation.rs"),
    )
    .expect("failed to read constraint_validation.rs");
    let compact_source = source.split_whitespace().collect::<String>();

    assert_eq!(
        source
            .matches("infer_result_constraint_relation_outcome(")
            .count(),
        6,
        "infer-result constraint validation should route infer base, evaluated result, positional, and hidden-base probes through the infer-result request"
    );
    assert!(
        !compact_source.contains("assign_relation_outcome(infer_base,inst_constraint)")
            && !compact_source.contains("assign_relation_outcome(evaluated,inst_constraint)")
            && !compact_source
                .contains("assign_relation_outcome(type_arg_evaluated,inst_constraint)")
            && !compact_source
                .contains("assign_relation_outcome(positional_constraint,inst_constraint)")
            && !compact_source.contains("assign_relation_outcome(hidden_base,inst_constraint)"),
        "infer-result constraint validation should not use raw assign RelationOutcome probes for infer-derived constraint checks"
    );
}
