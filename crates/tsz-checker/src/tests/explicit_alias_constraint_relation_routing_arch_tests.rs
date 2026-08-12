use std::fs;

#[test]
fn explicit_alias_constraint_uses_relation_outcome_boundary() {
    let source =
        fs::read_to_string("src/checkers/generic_checker/explicit_alias_constraint_helpers.rs")
            .expect("failed to read explicit_alias_constraint_helpers.rs");

    assert_eq!(
        source
            .matches("explicit_alias_constraint_relation_outcome")
            .count(),
        1,
        "explicit alias constraint compatibility should route through its dedicated request"
    );
    assert!(
        source.contains(".related"),
        "explicit alias constraint compatibility should use the relation outcome decision"
    );
    assert!(
        !source.contains("assign_relation_outcome")
            && !source.contains("diagnostic_relation_boolean_guard"),
        "explicit alias constraint compatibility should not regress to generic or raw relation guards"
    );
}

#[test]
fn successful_explicit_alias_constraint_relations_are_file_local_cached() {
    let relation_source =
        fs::read_to_string("src/assignability/explicit_alias_constraint_relation.rs")
            .expect("failed to read explicit_alias_constraint_relation.rs");
    let caches_source =
        fs::read_to_string("src/context/caches.rs").expect("failed to read caches.rs");
    let reset_source = fs::read_to_string("src/context/file_session_reset.rs")
        .expect("failed to read file_session_reset.rs");

    assert!(
        caches_source.contains("explicit_alias_constraint_relation_successes")
            && relation_source.contains("explicit_alias_constraint_relation_successes")
            && relation_source.contains("pack_relation_flags")
            && relation_source.contains("sound_mode")
            && relation_source.contains("outcome.related")
            && reset_source.contains("explicit_alias_constraint_relation_successes")
            && reset_source.contains(".clear()"),
        "successful explicit-alias constraint relation probes should be cached by \
         prepared source/target plus relation mode (#15729), mirroring the \
         type-argument constraint success cache, while failures keep the real \
         diagnostic relation path"
    );
    assert!(
        caches_source.contains("explicit_alias_relation_successes")
            && relation_source.contains("explicit_alias_relation_successes"),
        "successful explicit-alias constraint relations should also publish to the \
         program-wide SharedConstraintProofCache tier"
    );
}
