use std::fs;
use std::path::Path;

#[test]
fn indexed_access_constraint_uses_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/constraint_indexed_access_helpers.rs");
    let source = fs::read_to_string(&source_path).expect("read indexed-access helper source");

    let function_start = source
        .find("pub(super) fn constraint_check_indexed_access_value_type")
        .expect("find indexed-access constraint helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    pub(super) fn concrete_indexed_access_property_union")
        .expect("find next helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "indexed-access key-space relation decisions must use the shared relation outcome boundary"
    );
    assert!(
        !function.contains("assign_relation_outcome"),
        "indexed-access constraint key-space checks should route through named RelationRequests"
    );
    assert!(
        function.contains("indexed_access_constraint_key_relation_outcome("),
        "the keyed-object to object-keys relation should route through the indexed-access constraint request helper"
    );
}

#[test]
fn indexed_access_key_space_helpers_use_relation_outcome_boundary() {
    let source_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/types/computation/access_helpers.rs");
    let source = fs::read_to_string(&source_path).expect("read access helper source");

    let function_start = source
        .find("pub(crate) fn narrow_string_index_signature_rejects_index")
        .expect("find narrow string index helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    pub(crate) fn is_generic_key_space")
        .expect("find next helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "indexed-access key-space diagnostics must use the shared relation outcome boundary"
    );
    assert_eq!(
        function
            .matches("indexed_access_key_space_relation_outcome(")
            .count(),
        4,
        "string-index, constrained-keyof, union-member, and transformed-index key-space checks should route through the named RelationRequest"
    );
    assert!(
        !function.contains("assign_relation_outcome("),
        "indexed-access access-helper key-space checks must not regress to raw relation probes"
    );
}

#[test]
fn generic_index_policy_helpers_use_key_constraints_boundary() {
    let source_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/types/computation/access_helpers.rs");
    let source = fs::read_to_string(&source_path).expect("read access helper source");

    let helper_names = [
        "is_generic_index_type",
        "intersection_has_generic_index",
        "index_resolves_to_keyof_of_receiver",
        "is_valid_index_for_type_param",
        "same_type_param_identity",
        "type_contains_same_type_param_identity",
        "generic_index_mentions_transformed_current_type_param",
        "keyof_source_type_param",
        "is_generic_key_space",
    ];

    for helper_name in helper_names {
        let signature = if helper_name == "index_resolves_to_keyof_of_receiver"
            || helper_name == "same_type_param_identity"
            || helper_name == "type_contains_same_type_param_identity"
        {
            format!("fn {helper_name}")
        } else {
            format!("pub(crate) fn {helper_name}")
        };
        let helper_start = source
            .find(&signature)
            .unwrap_or_else(|| panic!("find generic index policy helper signature: {signature}"));
        let helper = &source[helper_start..];
        let helper_end = [
            helper.find("\n    fn "),
            helper.find("\n    pub(crate) fn "),
        ]
        .into_iter()
        .flatten()
        .filter(|&idx| idx > 0)
        .min()
        .unwrap_or(helper.len());
        let helper_body = &helper[..helper_end];

        assert!(
            !helper_body.contains("query_boundaries::common::"),
            "{helper_name} should route key/index shape policy through query_boundaries::key_constraints"
        );
        assert!(
            helper_body.contains("query_boundaries::key_constraints::"),
            "{helper_name} should call the key_constraints query boundary"
        );
    }
}

#[test]
fn indexed_access_type_checking_helpers_use_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/types/type_checking/indexed_access/indexed_access_helpers.rs");
    let source =
        fs::read_to_string(&source_path).expect("read indexed-access type-checking helper source");

    let helper_start = source
        .find("pub(super) fn type_literal_member_values_accept_index")
        .expect("find type literal indexed-access helper");
    let helper_end = source[helper_start..]
        .find("\n    fn keyof_candidate_target_is_array_like")
        .expect("find end of indexed-access key-space helper block");
    let helpers = &source[helper_start..helper_start + helper_end];
    let tuple_chain_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/types/type_checking/indexed_access/generic_tuple_chain.rs");
    let tuple_chain =
        fs::read_to_string(&tuple_chain_path).expect("read tuple-chain indexed-access helpers");
    let guarded_helpers = format!("{helpers}\n{tuple_chain}");
    let compact_helpers: String = helpers.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        !guarded_helpers.contains("diagnostic_relation_boolean_guard"),
        "indexed-access type-checking key-space helpers must use relation outcomes"
    );
    assert_eq!(
        guarded_helpers
            .matches("indexed_access_key_space_relation_outcome(")
            .count(),
        14,
        "indexed-access helper key-space probes should route through the named RelationRequest"
    );
    assert!(
        compact_helpers.contains(
            "indexed_access_key_space_relation_outcome(index_for_check,value_keyof).related"
        ),
        "type-literal member value checks should route index/keyof compatibility through RelationOutcome"
    );
    assert!(
        compact_helpers.contains(
            "indexed_access_key_space_relation_outcome(nested_index_for_check,nested_base_keyof).related"
        ),
        "nested type-literal indexed access checks should route through RelationOutcome"
    );
    assert!(
        compact_helpers
            .contains("indexed_access_key_space_relation_outcome(member,keyof_object).related"),
        "union index member checks should route through RelationOutcome"
    );
    assert!(
        compact_helpers.contains(
            "indexed_access_key_space_relation_outcome(index_type,template_keyof).related"
        ),
        "mapped constraint value checks should route through RelationOutcome"
    );
    assert!(
        compact_helpers
            .contains("indexed_access_key_space_relation_outcome(index_type,values_keyof).related"),
        "constraint value-keyof checks should route through RelationOutcome"
    );
    assert!(
        compact_helpers.contains(
            "indexed_access_key_space_relation_outcome(index_type_for_check,constraint_eval).related"
        ) && compact_helpers.contains(
            "indexed_access_key_space_relation_outcome(constraint_eval,index_type_for_check,).related"
        ),
        "mapped own-key constraint checks should route mutual compatibility through RelationOutcome"
    );
    assert!(
        compact_helpers.contains(
            "indexed_access_key_space_relation_outcome(candidate,string_or_number).related"
        ),
        "string-index candidate checks should route through RelationOutcome"
    );
    assert!(
        compact_helpers.contains(
            "indexed_access_key_space_relation_outcome(current_index_for_check,current_base_keyof).related"
        ),
        "deferred constraint-chain target checks should route through RelationOutcome"
    );
    assert!(
        !guarded_helpers.contains("assign_relation_outcome("),
        "indexed-access key-space helper probes should use named RelationRequests"
    );
}

#[test]
fn indexed_access_ts2536_key_space_checks_use_relation_outcome_boundary() {
    let source_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/types/type_checking/indexed_access.rs");
    let source = fs::read_to_string(&source_path).expect("read indexed-access checker source");
    let compact_source: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "indexed-access TS2536 key-space checks should not use raw diagnostic boolean relation guards"
    );
    assert!(
        !source.contains("assign_relation_outcome("),
        "indexed-access TS2536 key-space checks should use named RelationRequests"
    );
    assert!(
        compact_source.contains(
            "indexed_access_key_space_relation_outcome(constraint_eval,keyof_object).related"
        ),
        "constraint/keyof acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source.contains(
            "indexed_access_key_space_relation_outcome(check_index_eval,keyof_type).related"
        ),
        "type-literal fast-path index/keyof acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source.contains(
            "indexed_access_key_space_relation_outcome(index_type_for_check,keyof_object).related"
        ),
        "raw indexed-access key-space acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source.contains(
            "indexed_access_key_space_relation_outcome(next_evaluated,keyof_object).related"
        ),
        "transitive constraint-chain key-space acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source.contains(
            "indexed_access_key_space_relation_outcome(nested_index_for_check,constrained_base_keyof,).related"
        ),
        "nested indexed-access key-space acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source.contains(
            "indexed_access_key_space_relation_outcome(index_type_for_check,keyof_values,).related"
        ),
        "value-union keyof fallback checks should route through RelationOutcome"
    );
}

#[test]
fn indexed_access_computation_keyof_source_checks_use_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/types/computation/access.rs");
    let source = fs::read_to_string(&source_path).expect("read indexed-access computation source");
    let compact_source: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "indexed-access computation diagnostics should not use raw diagnostic boolean relation guards"
    );
    assert!(
        !source.contains("assign_relation_outcome("),
        "indexed-access computation diagnostics should use named RelationRequests"
    );
    assert!(
        compact_source.contains(
            "indexed_access_key_space_relation_outcome(pre_resolution_object_type,key_source,).related"
        ),
        "keyof-source type-parameter checks should route relation truth through the indexed-access key-space request"
    );
}
