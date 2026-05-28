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
    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        1,
        "the keyed-object to object-keys relation should route through RelationOutcome"
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
        function.matches("assign_relation_outcome").count(),
        3,
        "string-index, constrained-keyof, and union-member key-space checks should route through RelationOutcome"
    );
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
    let compact_helpers: String = helpers.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        !helpers.contains("diagnostic_relation_boolean_guard"),
        "indexed-access type-checking key-space helpers must use relation outcomes"
    );
    assert!(
        compact_helpers.contains("assign_relation_outcome(index_for_check,value_keyof).related"),
        "type-literal member value checks should route index/keyof compatibility through RelationOutcome"
    );
    assert!(
        compact_helpers
            .contains("assign_relation_outcome(nested_index_for_check,nested_base_keyof).related"),
        "nested type-literal indexed access checks should route through RelationOutcome"
    );
    assert!(
        compact_helpers.contains("assign_relation_outcome(member,keyof_object).related"),
        "union index member checks should route through RelationOutcome"
    );
    assert!(
        compact_helpers.contains("assign_relation_outcome(index_type,template_keyof).related"),
        "mapped constraint value checks should route through RelationOutcome"
    );
    assert!(
        compact_helpers.contains("assign_relation_outcome(candidate,string_or_number).related"),
        "string-index candidate checks should route through RelationOutcome"
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
        compact_source.contains("assign_relation_outcome(constraint_eval,keyof_object).related"),
        "constraint/keyof acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source.contains("assign_relation_outcome(check_index_eval,keyof_type).related"),
        "type-literal fast-path index/keyof acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source
            .contains("assign_relation_outcome(index_type_for_check,keyof_object).related"),
        "raw indexed-access key-space acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source.contains("assign_relation_outcome(next_evaluated,keyof_object).related"),
        "transitive constraint-chain key-space acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source.contains(
            "assign_relation_outcome(nested_index_for_check,constrained_base_keyof).related"
        ),
        "nested indexed-access key-space acceptance should route through RelationOutcome"
    );
    assert!(
        compact_source
            .contains("assign_relation_outcome(index_type_for_check,keyof_values).related"),
        "value-union keyof fallback checks should route through RelationOutcome"
    );
}
