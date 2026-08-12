use std::fs;
use std::path::Path;

#[test]
fn generic_constraint_validation_no_weak_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/checkers/generic_checker/constraint_validation.rs"),
    )
    .expect("failed to read constraint_validation.rs");

    // Four routed call sites: three landed independently on main, plus the
    // #14337 fallback that defers TS2344 when the type-argument constraint
    // stays an unresolved `Lazy` (added by this change).
    assert_eq!(
        source
            .matches("type_arg_constraint_no_weak_relation_outcome(")
            .count(),
        4,
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
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/cached_constraint_relation_helpers.rs"),
    )
    .expect("failed to read cached_constraint_relation_helpers.rs");
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
fn successful_explicit_alias_constraint_relations_are_file_local_cached() {
    let relation_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/cached_constraint_relation_helpers.rs"),
    )
    .expect("failed to read cached_constraint_relation_helpers.rs");
    let caches_source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/context/caches.rs"))
            .expect("failed to read caches.rs");
    let reset_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/context/file_session_reset.rs"),
    )
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
         prepared source/target plus relation mode (#15729), mirroring \
         type_arg_constraint_relation_successes, while failures keep the real \
         diagnostic relation path"
    );
}

#[test]
fn conditional_branch_constraint_proofs_use_stamped_typed_cache_keys() {
    let helper_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/checkers/generic_checker/conditional_constraint_helpers.rs"),
    )
    .expect("failed to read conditional_constraint_helpers.rs");
    let caches_source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/context/caches.rs"))
            .expect("failed to read caches.rs");
    let reset_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/context/file_session_reset.rs"),
    )
    .expect("failed to read file_session_reset.rs");

    assert!(
        caches_source.contains("GenericConstraintProofKey")
            && caches_source.contains("GenericConstraintProofMemo")
            && caches_source
                .contains("pub conditional_branch_constraint: GenericConstraintProofMemo")
            && caches_source
                .contains("pub indexed_object_map_branch_constraint: GenericConstraintProofMemo"),
        "conditional and indexed-object branch proof caches should use the typed, stamped TS2344 proof memo"
    );
    assert!(
        helper_source.contains("generic_constraint_proof_key(")
            && helper_source.contains("assignability_eval_memo_stamp()")
            && helper_source.contains("generic_constraint_proof_completed_clean(")
            && helper_source.contains(".conditional_branch_constraint")
            && helper_source.contains(".indexed_object_map_branch_constraint"),
        "branch proof helpers should key lookups by relation policy and stamp, then cache only clean results"
    );
    assert!(
        !caches_source.contains("conditional_branch_constraint: FxHashMap<(TypeId, TypeId), bool>")
            && !caches_source.contains(
                "indexed_object_map_branch_constraint: FxHashMap<(TypeId, TypeId), bool>"
            )
            && !helper_source
                .contains("conditional_branch_constraint\n                .get(&cache_key)")
            && !helper_source
                .contains("indexed_object_map_branch_constraint\n                .get(&cache_key)")
            && reset_source.contains("conditional_branch_constraint")
            && reset_source.contains("indexed_object_map_branch_constraint")
            && reset_source.contains(".clear()"),
        "branch proof caches must not regress to raw TypeId-pair maps and must keep the file-session reset boundary"
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
