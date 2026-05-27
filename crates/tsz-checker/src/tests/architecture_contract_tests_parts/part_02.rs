/// Solver failure normalization must preserve the canonical semantic-family
/// mapping we rely on throughout the checker.
#[test]
fn test_relation_failure_preserves_canonical_solver_mapping() {
    let source = fs::read_to_string("src/query_boundaries/relation_types.rs")
        .expect("failed to read query_boundaries/relation_types.rs");

    assert!(
        source.contains("SubtypeFailureReason::NoCommonProperties")
            && source.contains("=> Self::WeakUnionViolation"),
        "NoCommonProperties must normalize to WeakUnionViolation"
    );
    assert!(
        source.contains("SubtypeFailureReason::OptionalPropertyRequired { property_name }")
            && source.contains("Self::PropertyModifierMismatch { property_name }"),
        "OptionalPropertyRequired must normalize to PropertyModifierMismatch"
    );
    assert!(
        source.contains("SubtypeFailureReason::PropertyTypeMismatch")
            && source.contains("Self::IncompatiblePropertyValue"),
        "PropertyTypeMismatch must normalize to IncompatiblePropertyValue"
    );
    assert!(
        source.contains("nested: nested_reason.map(|r| Box::new(Self::from_solver_reason(*r)))"),
        "nested property/return mismatches must recurse through from_solver_reason"
    );
    assert!(
        source.contains("SubtypeFailureReason::NoUnionMemberMatches { source_type, .. }")
            && source.contains("target_type: TypeId::ERROR,"),
        "NoUnionMemberMatches must normalize to a TypeMismatch sentinel with target_type=TypeId::ERROR"
    );
    assert!(
        source.contains("SubtypeFailureReason::TypeMismatch")
            && source.contains("| SubtypeFailureReason::IntrinsicTypeMismatch")
            && source.contains("| SubtypeFailureReason::LiteralTypeMismatch")
            && source.contains("| SubtypeFailureReason::ErrorType")
            && source.contains("| SubtypeFailureReason::ReadonlyToMutableAssignment")
            && source.contains("| SubtypeFailureReason::NoIntersectionMemberMatches")
            && source.contains("} => Self::TypeMismatch {")
            && source.contains("source_type,")
            && source.contains("target_type,"),
        "direct type-mismatch solver reasons must normalize through the shared TypeMismatch passthrough"
    );
    assert!(
        source.contains("SubtypeFailureReason::MissingIndexSignature { .. }")
            && source.contains("| SubtypeFailureReason::RecursionLimitExceeded")
            && source.contains("=> Self::TypeMismatch {")
            && source.contains("source_type: TypeId::ERROR,")
            && source.contains("target_type: TypeId::ERROR,"),
        "MissingIndexSignature and RecursionLimitExceeded must normalize to a TypeMismatch sentinel with ERROR/ERROR"
    );
    assert!(
        source.contains("SubtypeFailureReason::ArrayElementMismatch {")
            && source.contains("source_type: source_element,")
            && source.contains("target_type: target_element,")
            && source.contains("SubtypeFailureReason::IndexSignatureMismatch {")
            && source.contains("source_type: source_value_type,")
            && source.contains("target_type: target_value_type,")
            && source.contains("SubtypeFailureReason::TupleElementTypeMismatch {")
            && source.contains("source_type: source_element,")
            && source.contains("target_type: target_element,"),
        "element/index-specific solver mismatches must normalize through TypeMismatch using the concrete element/value types"
    );
}

/// `RelationRequest` must keep the builder helpers that encode freshness and
/// spread policy directly into the canonical relation request shape.
#[test]
fn test_relation_request_builders_encode_epc_policy() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        source.contains("fn with_fresh_source"),
        "RelationRequest must keep with_fresh_source as the canonical fresh-literal builder"
    );
    assert!(
        source.contains("self.source_is_fresh = true;"),
        "with_fresh_source must mark the request as fresh"
    );
    assert!(
        source.contains("self.excess_property_mode = ExcessPropertyMode::Check;"),
        "with_fresh_source must enable full excess-property checking"
    );
    assert!(
        source.contains("fn with_spread_source"),
        "RelationRequest must keep with_spread_source as the canonical spread-literal builder"
    );
    assert!(
        source.contains("self.excess_property_mode = ExcessPropertyMode::CheckExplicitOnly;"),
        "with_spread_source must enable explicit-only excess-property checking"
    );
}

/// The canonical `RelationRequest` constructors must continue encoding the
/// semantic question directly as a `RelationKind`, rather than relying on
/// ambient caller-side policy.
#[test]
fn test_relation_request_constructors_encode_relation_kind() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    for (ctor, kind) in [
        ("fn assign", "RelationKind::Assign"),
        ("fn call_arg", "RelationKind::CallArg"),
        ("fn return_stmt", "RelationKind::Return"),
        ("fn satisfies", "RelationKind::Satisfies"),
        ("fn destructuring", "RelationKind::Destructuring"),
    ] {
        assert!(
            source.contains(ctor) && source.contains(kind),
            "{ctor} must construct a RelationRequest with {kind}"
        );
    }
}

/// `assignability_checker.rs` must use `execute_relation_request` as the
/// canonical checker-level entry point for structured relation queries.
#[test]
fn test_assignability_checker_has_execute_relation_request() {
    let source = fs::read_to_string("src/assignability/assignability_checker.rs")
        .expect("failed to read assignability_checker.rs");

    assert!(
        source.contains("fn execute_relation_request("),
        "assignability_checker must define execute_relation_request as the canonical \
         checker-level entry point for structured relation queries"
    );
    assert!(
        source.contains("execute_relation("),
        "execute_relation_request must delegate to the query_boundaries::execute_relation helper"
    );
    assert!(
        source
            .contains("checker_only_assignability_failure_reason(request.source, request.target)"),
        "execute_relation_request must preserve checker-only post-check downgrades \
         after the canonical boundary returns"
    );
    assert!(
        source.contains("outcome.related = false;"),
        "execute_relation_request must be able to downgrade a solver-related result \
         when checker-only semantics require it"
    );
    assert!(
        source.contains("let flags = self.ctx.pack_relation_flags();"),
        "execute_relation_request must pass packed checker relation flags into the boundary"
    );
    assert!(
        source.contains("let overrides = CheckerOverrideProvider::new(self, None);"),
        "execute_relation_request must construct a checker override provider for the boundary call"
    );
    assert!(
        source.contains("self.ctx.sound_mode(),"),
        "execute_relation_request must pass checker sound_mode into the boundary"
    );
    assert!(
        source.contains("&self.ctx.inheritance_graph,"),
        "execute_relation_request must pass the checker inheritance graph into the boundary"
    );
    assert!(
        source.contains("Some(&self.ctx),"),
        "execute_relation_request must pass checker context into the boundary \
         for structured failure analysis"
    );
}

/// `assignability_diagnostics.rs` diagnostic paths must use the relation
/// outcome's `weak_union_violation` hint instead of re-calling the solver.
#[test]
fn test_diagnostic_paths_use_relation_outcome_hint() {
    let source = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability_diagnostics.rs");

    // The `check_assignable_or_report_at` method should build a RelationRequest
    assert!(
        source.contains("RelationRequest::assign("),
        "check_assignable_or_report_at must build a RelationRequest::assign for the canonical path"
    );
    assert!(
        source.contains("execute_relation_request("),
        "check_assignable_or_report_at must call execute_relation_request"
    );
    assert!(
        source.contains("should_skip_weak_union_error_with_hint("),
        "diagnostic paths must use should_skip_weak_union_error_with_hint \
         to avoid re-calling the solver for weak-union detection"
    );
}

/// `check_argument_assignable_or_report` must use the canonical
/// `RelationRequest::call_arg` path for call-argument relation queries.
#[test]
fn test_call_arg_diagnostic_uses_canonical_relation_path() {
    let source = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability_diagnostics.rs");

    assert!(
        source.contains("RelationRequest::call_arg("),
        "check_argument_assignable_or_report must build a RelationRequest::call_arg \
         for the canonical call-argument relation path"
    );
}

/// `analyze_assignability_failure` should stay aligned with the canonical
/// checker gate path and preserve the array/tuple weak-type suppression
/// that prevents false TS2559 diagnostics.
#[test]
fn test_assignability_failure_analysis_stays_on_canonical_gate() {
    let source = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability_diagnostics.rs");

    assert!(
        source.contains("check_assignable_gate_with_overrides("),
        "analyze_assignability_failure must use check_assignable_gate_with_overrides \
         to stay aligned with canonical checker relation semantics"
    );
    assert!(
        source.contains("checker_only_assignability_failure_reason("),
        "analyze_assignability_failure must preserve checker-only failure downgrades"
    );
    assert!(
        source.contains("target_extends_array_or_tuple("),
        "analyze_assignability_failure must retain array/tuple weak-type suppression \
         for NoCommonProperties false positives"
    );
    assert!(
        source.contains("SubtypeFailureReason::NoCommonProperties"),
        "analyze_assignability_failure must explicitly gate NoCommonProperties \
         before emitting weak-type diagnostics"
    );
}

/// Interface/base property compatibility should route through the canonical
/// relation boundary instead of re-running local assignability + weak-union logic.
#[test]
fn test_class_query_boundary_uses_relation_request_for_property_mismatch() {
    let source =
        fs::read_to_string("src/query_boundaries/class.rs").expect("failed to read class.rs");

    assert!(
        source.contains("RelationRequest::assign("),
        "query_boundaries/class.rs must build a RelationRequest::assign \
         for property mismatch checks"
    );
    assert!(
        source.contains("execute_relation_request("),
        "query_boundaries/class.rs must use execute_relation_request \
         for property mismatch checks"
    );
    assert!(
        source.contains("should_skip_weak_union_error_with_outcome("),
        "query_boundaries/class.rs must use the structured RelationOutcome \
         when suppressing weak-union/excess-property diagnostics"
    );
}

// =============================================================================
// Phase 2: Object/property/call compatibility through canonical boundary
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

/// Bivariant callback relation checks must preserve solver overflow flags.
///
/// Structural rule: when the bivariant-callback relation hits the same
/// relation recursion guard as ordinary assignability, the checker boundary
/// must carry `depth_exceeded` / `iteration_exceeded` into `RelationOutcome`
/// so diagnostic selection can emit TS2321/TS2859 instead of a generic mismatch.
#[test]
fn test_bivariant_relation_boundary_preserves_overflow_flags() {
    let boundary_source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read assignability.rs");
    let checker_source = fs::read_to_string("src/assignability/assignability_checker.rs")
        .expect("failed to read assignability_checker.rs");

    assert!(
        boundary_source.contains(") -> tsz_solver::RelationResult")
            && boundary_source.contains("tsz_solver::RelationKind::AssignableBivariantCallbacks"),
        "bivariant relation helper must return the full solver RelationResult"
    );
    assert!(
        boundary_source.contains("(r.is_related(), r.depth_exceeded, r.iteration_exceeded)"),
        "execute_relation must forward bivariant callback depth/iteration overflow flags"
    );
    assert!(
        !boundary_source.contains("(r, false, false)"),
        "execute_relation must not erase bivariant callback overflow flags"
    );
    assert!(
        checker_source.contains(
            "self.propagate_overflow_flags(\n            relation_result.depth_exceeded,\n            relation_result.iteration_exceeded,\n        );"
        ) && checker_source.contains("let result = relation_result.is_related();"),
        "legacy bivariant boolean wrapper must merge overflow flags before returning/caching the bool"
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
        source.contains("let member = normalize_member(*member);")
            && source
                .contains("is_primitive_type(db, member) || is_type_parameter_like(db, member)"),
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
    let query_cache = tsz_solver::construction::QueryCache::new(&interner);
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
    let cache = crate::TypeCache::default();

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
// Ratchet guards: prevent architecture debt from growing
// =============================================================================

/// Guard that the `TEMPORARILY_ALLOWED` bypass list in the solver-imports test
/// does not silently grow. When someone wraps a solver API in `query_boundaries`,
/// they should remove it from `TEMPORARILY_ALLOWED`, shrinking the count.
/// Adding new bypasses requires updating this ceiling (which reviewers will see).
///
/// Current ceiling: 0 items — the bypass list is empty.
#[test]
fn test_temporarily_allowed_bypass_list_does_not_grow() {
    // The authoritative list lives in test_solver_imports_go_through_query_boundaries.
    // We cannot inspect it at runtime, so we count the items in source.
    let src = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/architecture_contract_tests.rs"),
    )
    .expect("failed to read architecture_contract_tests.rs");

    // Find the TEMPORARILY_ALLOWED block and count non-comment, non-empty entries
    let mut in_block = false;
    let mut count = 0usize;
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.contains("const TEMPORARILY_ALLOWED") {
            in_block = true;
            continue;
        }
        if in_block {
            if trimmed == "];" {
                break;
            }
            // Count lines that are quoted string entries (start with `"`)
            if trimmed.starts_with('"') {
                count += 1;
            }
        }
    }

    const CEILING: usize = 0;
    assert_eq!(
        count, CEILING,
        "TEMPORARILY_ALLOWED bypass list has grown to {count} items (ceiling: {CEILING}). \
         Do not add new solver import bypasses — create a query_boundaries wrapper instead. \
         If a wrapper was created, remove the old entry and lower CEILING in this test."
    );
}

/// Guard that direct type-construction calls (`interner.union()`, `interner.intersection()`,
/// `interner.object()`, `interner.array()`, `interner.tuple()`, `interner.function()`)
/// in checker source files outside `query_boundaries/` and `tests/` do not increase.
///
/// These calls bypass the `query_boundaries` layer and should be migrated to use
/// `flow_analysis::union_types()` or equivalent boundary helpers.
///
/// Current ceiling: 14 occurrences. This number must only decrease over time.
#[test]
fn test_direct_interner_type_construction_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    const CONSTRUCTION_METHODS: &[&str] = &[
        "interner.union(",
        "interner.intersection(",
        "interner.object(",
        "interner.array(",
        "interner.tuple(",
        "interner.function(",
    ];

    let mut violations = Vec::new();
    let mut total_count = 0usize;

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip excluded directories
        if rel.starts_with("tests/") || rel.starts_with("query_boundaries/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for method in CONSTRUCTION_METHODS {
                if line.contains(method) {
                    violations.push(format!("  {}:{}", rel, line_num + 1));
                    total_count += 1;
                }
            }
        }
    }

    // Ceiling: current count of direct interner type-construction calls.
    // This number must only shrink as calls are migrated to query_boundaries.
    const CEILING: usize = 0;
    assert!(
        total_count == CEILING,
        "Direct interner type-construction calls outside query_boundaries have increased \
         to {total_count} (ceiling: {CEILING}). Use query_boundaries helpers \
         (e.g., flow_analysis::union_types, ::array_type, ::tuple_type, ::intersection_types). \
         Current occurrences:\n{}",
        violations.join("\n")
    );
}

/// Guard that `error_reporter/` modules remain a pure diagnostic formatting layer.
/// They must not perform type construction (no `interner.union()`, `interner.object()`, etc.)
/// or type evaluation (no `TypeEvaluator::new()`, `TypeInstantiator::new()`).
///
/// Error reporters should only read type data and format diagnostics.
#[test]
fn test_error_reporter_does_not_perform_type_construction() {
    let error_reporter_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter");
    let mut files = Vec::new();
    walk_rs_files_recursive(&error_reporter_dir, &mut files);

    const FORBIDDEN_PATTERNS: &[(&[&str], &str)] = &[
        (
            &[
                "interner.union(",
                "interner.intersection(",
                "interner.object(",
                "interner.array(",
                "interner.tuple(",
                "interner.function(",
            ],
            "direct type construction via interner",
        ),
        (
            &["TypeEvaluator::new("],
            "type evaluation (should be in checker/query_boundaries)",
        ),
    ];

    let mut violations = Vec::new();
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for (patterns, description) in FORBIDDEN_PATTERNS {
                for pattern in *patterns {
                    if line.contains(pattern) {
                        violations.push(format!("  {}:{} — {}", rel, line_num + 1, description,));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "error_reporter modules must remain a pure formatting layer. \
         The following files contain forbidden patterns:\n{}",
        violations.join("\n")
    );
}

/// Guard that the number of checker source files exceeding ~2000 LOC does not increase.
///
/// Per CLAUDE.md section 12: "Checker files should stay under ~2000 LOC."
/// This ratchet captures the current state and prevents
/// regression. As files are split, this ceiling must be lowered.
///
/// Current ceiling: 35 files over 2000 lines. This number must only decrease over time.
#[test]
fn test_checker_file_size_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut oversized = Vec::new();
    let mut max_lines = 0usize;

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip test files — they are not subject to the LOC guideline
        if rel.starts_with("tests/") || rel.contains("/test") {
            continue;
        }

        let line_count = match fs::read_to_string(path) {
            Ok(s) => s.lines().count(),
            Err(_) => continue,
        };

        if line_count > max_lines {
            max_lines = line_count;
        }

        if line_count > 2000 {
            oversized.push(format!("  {rel} ({line_count} lines)"));
        }
    }

    // Ceiling: number of checker source files exceeding 2000 LOC.
    // This number must only shrink as files are split into smaller modules.
    // Current oversized files (as of 2026-04-03):
    //   checkers/call_checker/mod.rs, checkers/generic_checker/mod.rs,
    //   checkers/jsx/props/mod.rs, checkers/jsx/props/resolution.rs, checkers/jsx/props/validation.rs, checkers/jsx/orchestration/mod.rs,
    //   types/type_checking/duplicate_identifiers.rs, types/function_type.rs,
    //   types/queries/lib.rs, types/utilities/core.rs, types/computation/binary.rs,
    //   types/computation/identifier/mod.rs, types/computation/call/inner.rs,
    //   types/computation/object_literal/mod.rs, types/property_access_helpers/mod.rs,
    //   types/property_access_type/mod.rs, types/class_type/core.rs,
    //   types/class_type/constructor.rs,
    //   classes/class_checker.rs, classes/class_implements_checker/mod.rs,
    //   declarations/import/core/mod.rs, declarations/import/declaration.rs,
    //   state/variable_checking/core.rs,
    //   state/variable_checking/variable_helpers/mod.rs, state/variable_checking/destructuring.rs,
    //   state/type_analysis/computed_commonjs/mod.rs, state/type_analysis/computed/mod.rs,
    //   state/type_resolution/module.rs,
    //   jsdoc/params.rs, jsdoc/resolution/mod.rs, symbols/scope_finder.rs,
    //   assignability/assignment_checker/mod.rs, error_reporter/core/mod.rs,
    //   error_reporter/call_errors/mod.rs, flow/control_flow/core.rs
    const FILE_COUNT_CEILING: usize = 35;
    assert!(
        oversized.len() <= FILE_COUNT_CEILING,
        "Number of checker source files over 2000 LOC has grown to {} (ceiling: {FILE_COUNT_CEILING}). \
         Split oversized files into smaller modules before adding new code. \
         Current oversized files:\n{}",
        oversized.len(),
        oversized.join("\n")
    );

    // Ceiling: maximum line count of any single checker source file.
    // This prevents existing large files from growing further.
    // Bumped 3090→3095 for the narrowed-union receiver TS2339 display fix
    // (#1869); 3095→3105 for the globalThis property/element access TS7017/
    // TS7053 emission fix and intersection-annotation TS2339 receiver display;
    // 3105→3130 for contextual implicit-any deferral and class recovery guards;
    // 3130→3145 for generic assertion predicate instantiation fix (issue #5790);
    // 3145→3148 for the Kysely alias-identity included-alias assignability path;
    // 3148→3160 for call/inner.rs growth on main (pre-existing; track a split).
    const MAX_LOC_CEILING: usize = 3160;
    assert!(
        max_lines <= MAX_LOC_CEILING,
        "Largest checker source file has grown to {max_lines} lines (ceiling: {MAX_LOC_CEILING}). \
         Split the file into smaller modules. Current oversized files:\n{}",
        oversized.join("\n")
    );
}

/// CLAUDE.md §4: Lowering must not import Checker or Emitter.
/// tsz-lowering is a bridge from AST to solver types; it should only depend on
/// parser, binder, solver, and common. Importing the checker or emitter would
/// create a backwards dependency in the pipeline.
#[test]
fn test_lowering_must_not_import_checker_or_emitter() {
    let lowering_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-lowering/src");
    if !lowering_src.exists() {
        return;
    }

    let mut files = Vec::new();
    walk_rs_files_recursive(&lowering_src, &mut files);

    let forbidden_crates = ["tsz_checker", "tsz_emitter"];

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for crate_name in &forbidden_crates {
                if line.contains(&format!("use {crate_name}"))
                    || line.contains(&format!("{crate_name}::"))
                {
                    violations.push(format!(
                        "{}:{}: imports {}",
                        path.display(),
                        line_num + 1,
                        crate_name
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Lowering must not import Checker or Emitter (CLAUDE.md §4). \
         Lowering bridges AST to solver types; it should not depend on \
         downstream pipeline stages. Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// Guard that CLI and ancillary crates consume checker only through public API paths.
///
/// Per CLAUDE.md section 4: "CLI and ancillary crates must consume checker diagnostics
/// via `tsz_checker::diagnostics`."
///
/// This prevents the CLI from reaching into checker internals (types, state, flow,
/// checkers, symbols, etc.) which would create tight coupling.
#[test]
fn test_cli_must_not_import_checker_internals() {
    let cli_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-cli/src");
    if !cli_src.exists() {
        // Skip if CLI crate doesn't exist in this workspace layout
        return;
    }

    let mut files = Vec::new();
    walk_rs_files_recursive(&cli_src, &mut files);

    // These are checker-internal module paths that CLI must not import.
    // `tsz_checker::diagnostics` and `tsz_checker::context` are the allowed public API.
    const FORBIDDEN_IMPORTS: &[&str] = &[
        "tsz_checker::types::",
        "tsz_checker::state::",
        "tsz_checker::flow::",
        "tsz_checker::checkers::",
        "tsz_checker::symbols::",
        "tsz_checker::error_reporter::",
        "tsz_checker::declarations::",
    ];

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&cli_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for &forbidden in FORBIDDEN_IMPORTS {
                if line.contains(forbidden) {
                    violations.push(format!(
                        "  {}:{} — imports {}",
                        rel,
                        line_num + 1,
                        forbidden
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "CLI crate must not import checker internals. \
         Use `tsz_checker::diagnostics` for diagnostic codes and types. \
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Guard that the retired constructor boundary shortcut does not return.
#[test]
fn test_constructor_boundary_avoids_retired_construct_return_data_shortcut() {
    let source = fs::read_to_string("src/query_boundaries/checkers/constructor.rs")
        .expect("failed to read query_boundaries/checkers/constructor.rs");

    assert!(
        !source.contains("type_queries::data::construct_return_type_for_type"),
        "constructor query boundary must not call the retired solver data shortcut. \
         Use tsz_solver::type_queries::construct_return_type_for_type so the solver \
         query layer owns construct-return access."
    );
    assert!(
        source.contains("construct_return_type_for_type(db, type_id)"),
        "constructor query boundary must keep construct-return access routed through \
         its display helper."
    );
}

/// Guard that cleaned-up checker modules do not regress by re-introducing
/// direct `tsz_solver::type_queries::` calls (both `use` imports AND inline
/// fully-qualified calls).
///
/// ALL checker code outside `query_boundaries/` and `tests/` must use the
/// boundary wrappers in `query_boundaries/common.rs` instead of calling
/// `tsz_solver::type_queries::` directly. This is a blanket zero-tolerance guard.
#[test]
fn test_no_inline_type_queries_in_cleaned_modules() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if trimmed.contains("tsz_solver::type_queries::") {
                violations.push(format!("  {}:{} — {}", rel, line_num + 1, trimmed));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ALL checker code must use query_boundaries wrappers — no direct \
         tsz_solver::type_queries:: calls allowed outside query_boundaries/.\n\
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Zero-tolerance guard: no direct `tsz_solver::visitor::` calls are allowed outside
/// `query_boundaries/`. All visitor access must go through `query_boundaries::common`.
#[test]
fn test_no_inline_visitor_calls_in_checker_modules() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if trimmed.contains("tsz_solver::visitor::") {
                violations.push(format!("  {}:{} — {}", rel, line_num + 1, trimmed));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ALL checker code must use query_boundaries wrappers — no direct \
         tsz_solver::visitor:: calls allowed outside query_boundaries/.\n\
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Zero-tolerance guard: no direct inline calls to `tsz_solver::somefunc(` are allowed
/// outside `query_boundaries/`. All solver function calls must go through boundary wrappers.
///
/// This guard catches top-level solver function calls like `tsz_solver::is_conditional_type(...)`
/// that bypass the `query_boundaries` layer. Struct/enum paths like `tsz_solver::TypeId` and
/// sub-namespace paths like `tsz_solver::operations::property::` are excluded from this check
/// since they're either data types (handled by `test_solver_imports_go_through_query_boundaries`)
/// or internal solver modules with their own boundary guards.
#[test]
fn test_no_inline_solver_function_calls_in_checker_modules() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            // Detect `tsz_solver::lowercase_name(` — a direct solver function call.
            // This pattern matches `tsz_solver::` followed by a lowercase identifier (function)
            // and an opening paren, distinguishing it from type/struct paths.
            let mut rest = trimmed;
            while let Some(pos) = rest.find("tsz_solver::") {
                let after = &rest[pos + "tsz_solver::".len()..];
                // Check if this starts with a lowercase letter (function call, not type)
                if after
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
                {
                    // Check that there's no second `::` before a `(` — that would be a submodule
                    // path like `tsz_solver::operations::property::`, not a direct function call.
                    let name_end = after
                        .find(|c: char| !c.is_alphanumeric() && c != '_')
                        .unwrap_or(after.len());
                    let _name = &after[..name_end];
                    let suffix = &after[name_end..];
                    // It's a direct function call if followed immediately by `(`
                    if suffix.starts_with('(') {
                        violations.push(format!("  {}:{} — {}", rel, line_num + 1, trimmed));
                        break;
                    }
                }
                // Advance past this occurrence
                rest = &rest[pos + 1..];
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ALL checker code must use query_boundaries wrappers — no direct \
         inline tsz_solver::funcname( calls allowed outside query_boundaries/.\n\
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Ratchet guard: direct `tsz_solver::widening::widen_type` (or `operations::widening::`)
/// calls outside `query_boundaries/`, `tests/`, and `types/utilities/core.rs` must not grow.
///
/// Callers should use `query_boundaries::common::widen_type` (free function) or
/// `self.widen_literal_type()` (method on `CheckerState`) instead.
///
/// Current ceiling: 0 occurrences — all calls migrated to `query_boundaries`.
#[test]
fn test_direct_widening_calls_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut count = 0usize;
    let mut locations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip allowed locations
        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("tsz_solver::widening::widen_type")
                || line.contains("tsz_solver::operations::widening::widen_type")
            {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 0;
    assert!(
        count == CEILING,
        "Direct tsz_solver::widening::widen_type calls have grown to {count} (ceiling: {CEILING}). \
         Use query_boundaries::common::widen_type or self.widen_literal_type() instead.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Guard: no direct `expression_ops::` calls outside `query_boundaries/` and `tests/`.
///
/// Expression operation calls should go through `query_boundaries::type_computation::core`
/// wrappers to maintain the boundary layer.
#[test]
fn test_no_direct_expression_ops_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("expression_ops::") && line.contains("tsz_solver") {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct tsz_solver::expression_ops:: calls found outside query_boundaries/. \
         Use query_boundaries::type_computation::core wrappers instead.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

/// Guard: no direct `ApplicationEvaluator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// Application evaluation should go through boundary wrappers like
/// `query_boundaries::flow_analysis::evaluate_application_type`.
#[test]
fn test_no_direct_application_evaluator_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("ApplicationEvaluator::new(") {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct ApplicationEvaluator::new() calls found outside query_boundaries/. \
         Use query_boundaries::flow_analysis::evaluate_application_type instead.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

/// Guard: `context/def_mapping.rs` and context/speculation.rs must not cross-reference
/// each other. `def_mapping` owns SymbolId<->DefId identity mapping, speculation owns
/// checker state transaction boundaries. Mixing these concerns would violate the
/// clean context module separation (BOUNDARIES.md §4 Identity Boundary).
#[test]
fn test_def_mapping_and_speculation_do_not_cross_reference() {
    let def_mapping_src = fs::read_to_string("src/context/def_mapping.rs")
        .expect("failed to read src/context/def_mapping.rs");
    let speculation_src = fs::read_to_string("src/context/speculation.rs")
        .expect("failed to read src/context/speculation.rs");

    // def_mapping must not reference speculation types or functions
    assert!(
        !def_mapping_src.contains("DiagnosticSnapshot")
            && !def_mapping_src.contains("FullSnapshot")
            && !def_mapping_src.contains("ReturnTypeSnapshot")
            && !def_mapping_src.contains("rollback_")
            && !def_mapping_src.contains("snapshot_"),
        "def_mapping.rs must not reference speculation types or functions — \
         keep identity mapping separate from transaction boundaries"
    );

    // speculation must not reference def_mapping types or functions
    assert!(
        !speculation_src.contains("get_or_create_def_id")
            && !speculation_src.contains("def_mapping")
            && !speculation_src.contains("DefinitionStore")
            && !speculation_src.contains("DefinitionInfo"),
        "speculation.rs must not reference def_mapping types or functions — \
         keep transaction boundaries separate from identity mapping"
    );

    // Neither should perform type computation
    assert!(
        !def_mapping_src.contains("is_subtype_of") && !def_mapping_src.contains("is_assignable"),
        "def_mapping.rs must not perform type computation — it is pure identity mapping"
    );
    assert!(
        !speculation_src.contains("is_subtype_of") && !speculation_src.contains("is_assignable"),
        "speculation.rs must not perform type computation — it is pure state management"
    );
}

// =============================================================================
// Boundary Quarantine Tests — Evaluator/Checker Construction Ceilings
// =============================================================================

/// Guard: no `CompatChecker::new()` or `CompatChecker::with_resolver()` outside
/// `query_boundaries/` and `tests/`.
///
/// `CompatChecker` is the solver's Lawyer layer. Checker code should never construct
/// it directly — the relation should flow through `query_boundaries/assignability`
/// via `execute_relation()` and related helpers (CLAUDE.md §5, §22).
#[test]
fn test_no_direct_compat_checker_construction_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("CompatChecker::new(")
                || line.contains("CompatChecker::with_resolver(")
            {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct CompatChecker construction found outside query_boundaries/. \
         Route relation checks through query_boundaries/assignability instead (CLAUDE.md §5, §22).\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

/// Ceiling: direct `BinaryOpEvaluator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// These bypass the query boundary layer. A wrapper in
/// `query_boundaries/type_computation/core.rs` exists for `evaluate_plus_chain`;
/// more wrappers should be added over time. This ceiling must only decrease.
///
/// Current ceiling: 21 occurrences.
#[test]
fn test_direct_binary_op_evaluator_construction_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut count = 0usize;
    let mut locations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("BinaryOpEvaluator::new(") {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 0;
    assert!(
        count == CEILING,
        "BinaryOpEvaluator::new() usage ceiling exceeded: found {count} (ceiling: {CEILING}). \
         Use query_boundaries::common::new_binary_op_evaluator() instead.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Ceiling: direct `PropertyAccessEvaluator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// These bypass the query boundary layer. Wrappers should be created in
/// `query_boundaries/` over time. This ceiling must only decrease.
///
/// Current ceiling: 0 occurrences (all migrated to `query_boundaries`).
#[test]
fn test_direct_property_access_evaluator_construction_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut count = 0usize;
    let mut locations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("PropertyAccessEvaluator::new(") {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        count == 0,
        "PropertyAccessEvaluator::new() must not be used outside query_boundaries/. \
         Use query_boundaries::property_access::resolve_property_access instead. \
         Found {count} violations:\n{}",
        locations.join("\n")
    );
}

/// Ceiling: direct `TypeInstantiator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// Type instantiation should flow through `query_boundaries/common::instantiate_type`
/// or dedicated boundary helpers. This ceiling must only decrease.
///
/// Current ceiling: 1 occurrence (types/queries/lib.rs).
#[test]
fn test_direct_type_instantiator_construction_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut count = 0usize;
    let mut locations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("TypeInstantiator::new(") {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 0;
    assert!(
        count == CEILING,
        "TypeInstantiator::new() usage ceiling exceeded: found {count} (ceiling: {CEILING}). \
         Use query_boundaries/common::instantiate_type or create a new boundary wrapper.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Guard: no direct `tsz_solver::relations::freshness::` calls outside
/// `query_boundaries/` and `tests/`.
///
/// Freshness queries (`is_fresh_object_type`, `widen_freshness`) have wrappers
/// in `query_boundaries/common.rs`. All checker code must use those wrappers
/// to maintain the boundary between checker (WHERE) and solver (WHAT).
#[test]
fn test_no_direct_freshness_calls_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("tsz_solver::relations::freshness") {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct tsz_solver::relations::freshness:: calls found outside query_boundaries/. \
         Use query_boundaries::common::is_fresh_object_type / widen_freshness instead.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

// =============================================================================
// Stable Identity Helper Tests — DefId Resolution
// =============================================================================

/// Guard: core.rs must NOT contain inline type-param priming loops.
///
/// The ad hoc block that manually iterated symbol declarations to extract
/// type parameters was replaced by `ensure_def_ready_for_lowering`. This
/// test ensures the inline pattern doesn't regrow.
#[test]
fn test_core_type_resolution_uses_stable_identity_helper_for_type_param_priming() {
    let src = fs::read_to_string("src/state/type_resolution/core.rs")
        .expect("failed to read src/state/type_resolution/core.rs");

    // The old ad hoc pattern iterated declarations with get_interface + get_type_alias
    // inline to extract type parameters. This should now go through
    // ensure_def_ready_for_lowering which delegates to
    // extract_declared_type_params_for_reference_symbol.
    let has_inline_iface_param_extraction = src
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .any(|line| {
            line.contains("get_interface(node)")
                && !line.contains("ensure_def")
                && !line.contains("extract_declared")
        });

    assert!(
        !has_inline_iface_param_extraction,
        "core.rs contains inline interface type-param extraction. \
         Use ensure_def_ready_for_lowering (which delegates to \
         extract_declared_type_params_for_reference_symbol) instead."
    );
}

/// Guard: core.rs type reference resolution must delegate to
/// `ensure_def_ready_for_lowering` for generic ref type-param priming.
#[test]
fn test_core_type_resolution_has_ensure_def_ready_call() {
    let src = fs::read_to_string("src/state/type_resolution/core.rs")
        .expect("failed to read src/state/type_resolution/core.rs");

    assert!(
        src.contains("ensure_def_ready_for_lowering"),
        "core.rs must call ensure_def_ready_for_lowering for generic type \
         reference resolution. This is the stable-identity helper that \
        replaces ad hoc type-param priming blocks."
    );
}

/// Guard: `instanceof` narrowing for class and global-constructor symbols must
/// use real `DefId`-backed lazy types rather than raw SymbolId-shaped
/// `reference(SymbolRef)` fallback.
#[test]
fn test_instanceof_constructor_branches_avoid_raw_symbol_reference_fallback() {
    let source = fs::read_to_string("src/flow/control_flow/narrowing.rs")
        .expect("failed to read src/flow/control_flow/narrowing.rs");
    let class_branch = source
        .split("if symbol.has_any_flags(symbol_flags::CLASS)")
        .nth(1)
        .and_then(|rest| rest.split("// Global constructor variables").next())
        .expect("failed to isolate instanceof class-symbol branch");

    assert!(
        class_branch.contains("self.resolve_symbol_to_lazy(symbol_ref)"),
        "instanceof class-symbol branch should resolve through the DefId-backed lazy helper"
    );
    assert!(
        !class_branch.contains(".reference("),
        "instanceof class-symbol branch must not create Lazy(DefId(symbol_id)) via raw SymbolRef fallback"
    );

    let global_constructor_branch = source
        .split("// Global constructor variables")
        .nth(1)
        .and_then(|rest| rest.split("// For FUNCTION symbols").next())
        .expect("failed to isolate instanceof global-constructor branch");

    assert!(
        global_constructor_branch.contains("self.resolve_symbol_to_lazy(symbol_ref)"),
        "instanceof global-constructor branch should resolve through the DefId-backed lazy helper"
    );
    assert!(
        !global_constructor_branch.contains(".reference("),
        "instanceof global-constructor branch must not create Lazy(DefId(symbol_id)) via raw SymbolRef fallback"
    );
}

/// Guard: the manual `ArrayBuffer.isView` fallback must use real `DefId`-backed
/// lazy types rather than raw SymbolId-shaped `reference(SymbolRef)` fallback.
#[test]
fn test_array_buffer_is_view_avoids_raw_symbol_reference_fallback() {
    let source = fs::read_to_string("src/flow/control_flow/type_guards.rs")
        .expect("failed to read src/flow/control_flow/type_guards.rs");
    let branch = source
        .split("if type_id.is_none()")
        .nth(1)
        .and_then(|rest| rest.split("let type_id = type_id?;").next())
        .expect("failed to isolate ArrayBuffer.isView manual fallback branch");

    assert!(
        branch.contains("self.resolve_symbol_to_lazy(symbol_ref)?"),
        "ArrayBuffer.isView fallback should resolve ArrayBufferView through the DefId-backed lazy helper"
    );
    assert!(
        branch.contains("self.resolve_symbol_to_lazy(array_buffer_like_ref)?"),
        "ArrayBuffer.isView fallback should resolve ArrayBufferLike through the DefId-backed lazy helper"
    );
    assert!(
        !branch.contains(".reference("),
        "ArrayBuffer.isView fallback must not create Lazy(DefId(symbol_id)) via raw SymbolRef fallback"
    );
}

/// Guard: checker code must not add new raw `reference(SymbolRef)` fallback
/// construction. New checker code should resolve symbols through stable
/// `DefId` helpers before creating `Lazy(DefId)`.
#[test]
fn test_checker_raw_symbol_reference_construction_budget() {
    fn allowed_raw_reference_constructions(_rel_path: &str) -> usize {
        0
    }

    fn is_raw_reference_construction(line: &str) -> bool {
        let trimmed = line.trim_start();
        !trimmed.starts_with("//") && line.contains(".reference(")
    }

    let mut files = Vec::new();
    collect_checker_rs_files_recursive(Path::new("src"), &mut files);

    let mut violations = Vec::new();
    for path in files {
        if path
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }

        let rel_path = path.display().to_string();
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        let count = source
            .lines()
            .filter(|line| is_raw_reference_construction(line))
            .count();
        let allowed = allowed_raw_reference_constructions(&rel_path);

        if count > allowed {
            violations.push(format!(
                "{rel_path}: {count} raw .reference() calls (allowed {allowed})"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "new raw SymbolRef-backed reference construction found in checker code. \
         Resolve symbols through TypeEnvironment/DefId helpers before creating \
         Lazy(DefId), or migrate one of the existing allowlisted fallbacks first:\n{}",
        violations.join("\n")
    );
}

/// Guard: `reference_helpers.rs` must expose `ensure_def_ready_for_lowering`.
///
/// This helper consolidates the DefId + type-param + body priming pattern.
#[test]
fn test_reference_helpers_expose_stable_identity_helper() {
    let src = fs::read_to_string("src/state/type_resolution/reference_helpers.rs")
        .expect("failed to read src/state/type_resolution/reference_helpers.rs");

    assert!(
        src.contains("fn ensure_def_ready_for_lowering"),
        "reference_helpers.rs must expose ensure_def_ready_for_lowering — \
         the stable-identity helper for DefId + type-param + body priming."
    );
}

