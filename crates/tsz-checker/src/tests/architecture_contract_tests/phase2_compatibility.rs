use super::*;
// =============================================================================

/// `RelationOutcome` must include `property_classification` for structured
/// property-level analysis, avoiding checker-local re-derivation.
#[test]
fn test_relation_outcome_has_property_classification() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("property_classification:"),
        "RelationOutcome must include property_classification field \
         for structured property-level analysis"
    );

    assert!(
        source.contains("classify_object_properties("),
        "execute_relation must populate property_classification via \
         classify_object_properties boundary function"
    );
    assert!(
        source.contains("suppress_excess_property_failure_if_needed("),
        "execute_relation must centralize excess-property suppression through \
         suppress_excess_property_failure_if_needed"
    );
    assert!(
        source.contains("let property_classification =")
            && source.contains(
                "classify_object_properties(db.as_type_database(), request.source, request.target)"
            ),
        "execute_relation must always compute canonical property classification on failed relations"
    );
    assert!(
        source.contains("let (weak_union_violation, failure) = match analysis"),
        "execute_relation must derive weak-union and structured failure data together \
         from the same boundary analysis result"
    );
}

/// Successful relation results should return a clean `RelationOutcome`
/// with no leftover failure metadata attached.
#[test]
fn test_execute_relation_success_path_returns_clean_outcome() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("if related {")
            && source.contains("related: true,")
            && source.contains("failure: None,")
            && source.contains("weak_union_violation: false,")
            && source.contains("property_classification: None,"),
        "execute_relation success path must return a clean RelationOutcome \
         with no failure, weak-union, or property-classification residue"
    );
}

/// Failed relation results should return a structured `RelationOutcome`
/// that keeps the normalized failure facts attached.
#[test]
fn test_execute_relation_failure_path_returns_structured_outcome() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("RelationOutcome {")
            && source.contains("related: false,")
            && source.contains("failure,")
            && source.contains("weak_union_violation,")
            && source.contains("property_classification,"),
        "execute_relation failure path must return a structured RelationOutcome \
         with related=false plus failure, weak-union, and property-classification facts"
    );
}

/// The boundary must own the canonical excess-property suppression policy that
/// used to be duplicated in checker-local failure analysis.
#[test]
fn test_boundary_owns_excess_property_suppression_policy() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("fn suppress_excess_property_failure_if_needed("),
        "assignability boundary must define suppress_excess_property_failure_if_needed"
    );
    assert!(
        source.contains("has_deferred_conditional_member"),
        "boundary excess-property suppression must handle deferred conditional members"
    );
    assert!(
        source.contains("get_intersection_members"),
        "boundary excess-property suppression must inspect intersection members"
    );
    assert!(
        source.contains("is_primitive_type(db, *member) || is_type_parameter_like(db, *member)"),
        "boundary excess-property suppression must skip EPC for primitive/type-parameter \
         intersection members"
    );
}

/// `PropertyClassification` must exist in `relation_types.rs` as the canonical
/// property-level boundary output type.
#[test]
fn test_property_classification_exists() {
    let source = fs::read_to_string("src/query_boundaries/relation_types.rs")
        .expect("failed to read relation_types.rs");

    assert!(
        source.contains("pub(crate) struct PropertyClassification"),
        "relation_types.rs must define PropertyClassification as the canonical \
         boundary output for property-level analysis"
    );

    for field in [
        "excess_properties",
        "missing_properties",
        "incompatible_properties",
        "target_has_index_signature",
        "target_is_type_parameter",
        "target_is_empty_object",
        "target_is_global_object_or_function",
        "all_matching_compatible",
        "trimmed_source_assignable",
        "target_has_number_index",
    ] {
        assert!(
            source.contains(field),
            "PropertyClassification must include the `{field}` field"
        );
    }
}

/// `source_has_excess_properties` in `property.rs` must delegate to the
/// canonical boundary function instead of re-implementing shape analysis.
#[test]
fn test_source_has_excess_properties_uses_boundary() {
    let source = fs::read_to_string("src/state/state_checking/property.rs")
        .expect("failed to read property.rs");

    assert!(
        source.contains("classify_object_properties("),
        "source_has_excess_properties must delegate to classify_object_properties \
         boundary function instead of re-implementing property enumeration"
    );
}

/// The simple object target path in `check_object_literal_excess_properties`
/// must use the boundary classification for the excess-property decision.
#[test]
fn test_simple_object_epc_uses_boundary_classification() {
    let source = fs::read_to_string("src/state/state_checking/property.rs")
        .expect("failed to read property.rs");

    // The simple object target path should use classify_object_properties
    assert!(
        source.contains("classify_object_properties("),
        "check_object_literal_excess_properties simple-object path must use \
         classify_object_properties boundary for property existence decisions"
    );

    // The is_global_object_or_function_shape should delegate to boundary
    assert!(
        source.contains("is_global_object_or_function_shape_boundary("),
        "is_global_object_or_function_shape must delegate to the boundary function"
    );
}

/// The boundary must own the canonical `is_global_object_or_function_shape` logic.
#[test]
fn test_boundary_owns_global_object_function_shape_check() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");

    assert!(
        source.contains("fn is_global_object_or_function_shape("),
        "assignability.rs boundary must own is_global_object_or_function_shape"
    );
    assert!(
        source.contains("OBJECT_PROTO"),
        "boundary must contain the canonical Object.prototype property list"
    );
    assert!(
        source.contains("FUNCTION_PROTO"),
        "boundary must contain the canonical Function.prototype property list"
    );
}

/// `property.rs` must NOT contain its own `OBJECT_PROTO/FUNCTION_PROTO` lists.
/// These must be defined only in the boundary.
#[test]
fn test_property_rs_no_duplicate_proto_lists() {
    let source = fs::read_to_string("src/state/state_checking/property.rs")
        .expect("failed to read property.rs");

    assert!(
        !source.contains("OBJECT_PROTO"),
        "property.rs must NOT define OBJECT_PROTO — it must use the boundary"
    );
    assert!(
        !source.contains("FUNCTION_PROTO"),
        "property.rs must NOT define FUNCTION_PROTO — it must use the boundary"
    );
}

/// Verify that `CheckerState::with_cache_and_shared_def_store` propagates
/// the shared `DefinitionStore` to the checker context.
#[test]
fn test_shared_def_store_propagated_through_cache_constructor() {
    use std::sync::Arc;
    use tsz_solver::def::DefinitionStore;

    let shared_store = Arc::new(DefinitionStore::new());

    // Register a definition in the shared store so we can verify identity.
    let info = tsz_solver::def::DefinitionInfo::type_alias(
        tsz_common::interner::Atom(42),
        vec![],
        TypeId::STRING,
    );
    let def_id = shared_store.register(info);

    let interner = TypeInterner::new();
    let query_cache = tsz_solver::QueryCache::new(&interner);
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), "let x = 1;".to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let options = CheckerOptions {
        strict: false,
        ..Default::default()
    };

    // Create an empty TypeCache.
    let cache = crate::TypeCache {
        symbol_types: Default::default(),
        symbol_instance_types: Default::default(),
        node_types: Default::default(),
        symbol_dependencies: Default::default(),
        def_to_symbol: Default::default(),
        def_to_name: Default::default(),
        def_types: Default::default(),
        def_type_params: Default::default(),
        flow_analysis_cache: Default::default(),
        class_instance_type_to_decl: Default::default(),
        class_instance_type_cache: Default::default(),
        class_constructor_type_cache: Default::default(),
        type_only_nodes: Default::default(),
        namespace_module_names: Default::default(),
    };

    // Create checker with cache + shared def store.
    let checker = crate::state::CheckerState::with_cache_and_shared_def_store(
        arena,
        &binder,
        &query_cache,
        "test.ts".to_string(),
        cache,
        options,
        Arc::clone(&shared_store),
    );

    // The checker's definition store should be the same Arc instance.
    assert!(
        checker.ctx.definition_store.contains(def_id),
        "Checker should see definitions from the shared store"
    );
}

// =============================================================================
