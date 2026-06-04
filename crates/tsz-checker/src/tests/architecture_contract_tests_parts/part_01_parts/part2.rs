/// Ensures that `current_callable_type` is not reintroduced as ambient mutable state.
///
/// The callable type is now threaded explicitly via `CallableContext` through the call
/// argument collection pipeline. No file in the call-context lane should read or write
/// `ctx.current_callable_type`. The field has been removed from `CheckerContext`.
#[test]
fn no_ambient_current_callable_type() {
    let migrated_files = [
        "src/checkers/call_checker/mod.rs",
        "src/checkers/call_checker/applicability.rs",
        "src/checkers/call_checker/candidate_collection.rs",
        "src/checkers/call_checker/diagnostics.rs",
        "src/checkers/call_checker/overload_resolution.rs",
        "src/types/computation/call/mod.rs",
        "src/types/computation/call_inference.rs",
        "src/types/computation/call_display.rs",
        "src/state/type_analysis/computed_helpers.rs",
        "src/context/mod.rs",
        "src/context/constructors.rs",
    ];

    let checker_root = Path::new(env!("CARGO_MANIFEST_DIR"));

    for file in migrated_files {
        let path = checker_root.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        // Allow the doc comment in CallableContext's definition but forbid actual usage.
        // Filter out lines that are comments (starting with /// or //).
        let non_comment_lines: String = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.starts_with("///") && !trimmed.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            !non_comment_lines.contains("current_callable_type"),
            "File {file} must not reference `current_callable_type` — \
             use explicit `CallableContext` threading instead"
        );
    }
}

/// Excess property classification logic (`ExcessPropertiesKind` pattern-matching)
/// must stay in the canonical path: `state/state_checking/property.rs` and
/// the `query_boundaries/assignability.rs` re-export.  Other checker files
/// must not reimplement this classification.
#[test]
fn test_excess_property_classification_quarantined_to_property_rs() {
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(Path::new("src"), &mut files);

    let forbidden = [
        "ExcessPropertiesKind::Union",
        "ExcessPropertiesKind::Intersection",
        "ExcessPropertiesKind::Object(",
        "ExcessPropertiesKind::ObjectWithIndex(",
    ];

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        let allowed = rel.ends_with("state/state_checking/property.rs")
            || rel.ends_with("query_boundaries/assignability.rs")
            || rel.ends_with("assignability/assignability_diagnostics.rs") // target scoring
            || rel.ends_with("types/computation/object_literal_context.rs") // contextual type decomposition
            || rel.contains("/tests/");
        if allowed {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for pattern in &forbidden {
            if src.contains(pattern) {
                violations.push(format!("{rel} contains {pattern}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ExcessPropertiesKind pattern-matching must stay in state/state_checking/property.rs; violations:\n{}",
        violations.join("\n")
    );
}

/// The `query_boundaries/assignability.rs` boundary must expose the unified
/// `execute_relation` helper and the `RelationOutcome` / `RelationRequest`
/// types that the checker uses for single-pass relation + failure collection.
#[test]
fn test_relation_request_and_outcome_live_in_query_boundaries() {
    let boundary_source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");
    let request_source = fs::read_to_string("src/query_boundaries/relation_request.rs")
        .expect("failed to read query_boundaries/relation_request.rs");

    assert!(
        boundary_source.contains("pub(crate) use super::relation_request")
            && request_source.contains("pub(crate) struct RelationRequest"),
        "RelationRequest must be exposed through assignability and defined in query_boundaries/relation_request.rs"
    );
    assert!(
        boundary_source.contains("pub(crate) struct RelationOutcome"),
        "RelationOutcome must be defined in query_boundaries/assignability.rs"
    );
    assert!(
        boundary_source.contains("pub(crate) fn execute_relation"),
        "execute_relation boundary helper must be defined in query_boundaries/assignability.rs"
    );

    // RelationRequest must encode all policy dimensions
    assert!(
        request_source.contains("pub kind: RelationKind"),
        "RelationRequest must include a RelationKind field"
    );
    assert!(
        request_source.contains("pub excess_property_mode: ExcessPropertyMode"),
        "RelationRequest must include an ExcessPropertyMode field"
    );
    assert!(
        request_source.contains("pub missing_property_mode: MissingPropertyMode"),
        "RelationRequest must include a MissingPropertyMode field"
    );
    assert!(
        request_source.contains("pub source_is_fresh: bool"),
        "RelationRequest must include a source_is_fresh field"
    );

    // RelationOutcome must carry structured failure info
    assert!(
        boundary_source.contains("pub related: bool"),
        "RelationOutcome must include a `related` field"
    );
    assert!(
        boundary_source.contains("pub weak_union_violation: bool"),
        "RelationOutcome must include a `weak_union_violation` field"
    );
    assert!(
        boundary_source.contains("pub failure: Option<super::relation_types::RelationFailure>"),
        "RelationOutcome must include a structured `failure` field"
    );
}

/// The canonical request surface must continue exposing the full relation and
/// property-policy enum vocabulary, not implicit booleans.
#[test]
fn test_relation_request_policy_enums_cover_canonical_modes() {
    let source = fs::read_to_string("src/query_boundaries/relation_request.rs")
        .expect("failed to read query_boundaries/relation_request.rs");

    for variant in [
        "Assign",
        "ForInLhs",
        "CallArg",
        "Return",
        "JsxProps",
        "Destructuring",
        "RestParameter",
        "ImportAttributes",
        "ComputedEnumMember",
        "TypeParameterDefault",
        "IndexSignature",
        "DecoratorCallee", "JsdocTypeConstraint", "PropertyIndexKey", "NullishErrorTarget", "DuplicateIdentifier", "VariableInitializer", "DiagnosticSourceNarrowing", "ClassImplementsIndexValue", "ClassImplementsWholeType", "InterfaceHeritageIndexValue", "InterfaceHeritageGenericMethod", "InterfaceHeritagePropertyIndex", "JsdocHeritageConstraint", "MissingPropertyRead", "MissingPropertyWrite", "ExactOptionalSourceFilter", "JsxRenderFallback", "ObjectLiteralComputedKey", "ContextualSymbolIndexValue", "InOperatorKey", "InOperatorPrimitiveConstraint", "CompoundAssignment", "GenericElementWrite", "PropertyReceiverElementDisplay", "PropertyReceiverIndexValueDisplay", "ElementAccessNumberIndex", "ElementAccessMethodSuggestion", "CallElaborationMutual", "CallDisplayOverlap", "CallGeneratorYield", "CallAdapterCompatibility", "CallAdapterIdentity", "OverloadImplementationParameter",
        "Satisfies",
    ] {
        assert!(
            source.contains(&"enum RelationKind".to_string()) && source.contains(variant),
            "RelationKind must include the `{variant}` variant"
        );
    }

    for variant in ["Skip", "Check", "CheckExplicitOnly"] {
        assert!(
            source.contains(&"enum ExcessPropertyMode".to_string()) && source.contains(variant),
            "ExcessPropertyMode must include the `{variant}` variant"
        );
    }

    for variant in ["Report", "Suppress"] {
        assert!(
            source.contains(&"enum MissingPropertyMode".to_string()) && source.contains(variant),
            "MissingPropertyMode must include the `{variant}` variant"
        );
    }
}

/// The canonical `RelationRequest::new` path must keep request policy defaults
/// explicit at the boundary instead of relying on ambient caller state.
#[test]
fn test_relation_request_new_encodes_default_policy() {
    let source = fs::read_to_string("src/query_boundaries/relation_request.rs")
        .expect("failed to read query_boundaries/relation_request.rs");

    assert!(
        source.contains("fn new(source: TypeId, target: TypeId, kind: RelationKind) -> Self"),
        "RelationRequest must keep a canonical new(...) constructor for default policy"
    );
    assert!(
        source.contains("excess_property_mode: ExcessPropertyMode::Skip,"),
        "RelationRequest::new must default excess_property_mode to Skip"
    );
    assert!(
        source.contains("missing_property_mode: MissingPropertyMode::Report,"),
        "RelationRequest::new must default missing_property_mode to Report"
    );
    assert!(
        source.contains("source_is_fresh: false,"),
        "RelationRequest::new must default source_is_fresh to false"
    );
}

/// The canonical request builders must preserve explicit override hooks for
/// excess-property and missing-property policy at the boundary.
#[test]
fn test_relation_request_override_builders_remain_explicit() {
    let source = fs::read_to_string("src/query_boundaries/relation_request.rs")
        .expect("failed to read query_boundaries/relation_request.rs");

    assert!(
        source.contains("fn with_excess_property_mode(mut self, mode: ExcessPropertyMode) -> Self"),
        "RelationRequest must keep with_excess_property_mode as the explicit EPC override hook"
    );
    assert!(
        source.contains("self.excess_property_mode = mode;"),
        "with_excess_property_mode must write the requested EPC mode into the request"
    );
    assert!(
        source
            .contains("fn with_missing_property_mode(mut self, mode: MissingPropertyMode) -> Self"),
        "RelationRequest must keep with_missing_property_mode as the explicit missing-property override hook"
    );
    assert!(
        source.contains("self.missing_property_mode = mode;"),
        "with_missing_property_mode must write the requested missing-property mode into the request"
    );
}

/// The boundary-owned `RelationFlags` wrapper must continue exposing the
/// checker-safe flag surface for request-sensitive relation policy.
#[test]
fn test_relation_flags_surface_covers_checker_policy_bits() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        source.contains("pub(crate) struct RelationFlags;"),
        "assignability boundary must define RelationFlags as the checker-safe flag surface"
    );

    for flag in [
        "STRICT_NULL_CHECKS",
        "STRICT_FUNCTION_TYPES",
        "EXACT_OPTIONAL_PROPERTY_TYPES",
        "NO_UNCHECKED_INDEXED_ACCESS",
        "NO_ERASE_GENERICS",
        "ALLOW_BIVARIANT_REST",
    ] {
        assert!(
            source.contains(flag),
            "RelationFlags must expose the `{flag}` constant"
        );
    }
}

/// The checker boundary's flag wrapper should mirror the solver's typed
/// `RelationFlags` bit surface, not the legacy cache-key `FLAG_*` protocol.
#[test]
fn test_relation_flags_surface_uses_solver_typed_flags() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        source.contains("tsz_solver::RelationFlags::STRICT_NULL_CHECKS"),
        "RelationFlags wrapper must derive checker policy bits from solver typed flags"
    );
    assert!(
        !source.contains("tsz_solver::RelationCacheKey::FLAG_"),
        "RelationFlags wrapper must not depend on legacy RelationCacheKey FLAG_* constants"
    );
}

/// Checker compiler-option packing must stay on the boundary-owned
/// `RelationFlags` wrapper rather than reaching into solver internals.
#[test]
fn test_pack_relation_flags_uses_boundary_relation_flags_surface() {
    let source = fs::read_to_string("src/context/compiler_options.rs")
        .expect("failed to read context/compiler_options.rs");

    assert!(
        source.contains("use crate::query_boundaries::assignability::RelationFlags;"),
        "pack_relation_flags must import boundary-owned RelationFlags"
    );

    for flag in [
        "RelationFlags::STRICT_NULL_CHECKS",
        "RelationFlags::STRICT_FUNCTION_TYPES",
        "RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES",
        "RelationFlags::NO_UNCHECKED_INDEXED_ACCESS",
        "RelationFlags::ALLOW_BIVARIANT_REST",
    ] {
        assert!(
            source.contains(flag),
            "pack_relation_flags must use `{flag}` when encoding checker policy"
        );
    }

    assert!(
        !source.contains("RelationCacheKey::FLAG_STRICT_NULL_CHECKS"),
        "pack_relation_flags must not reach directly into RelationCacheKey bits"
    );
}

/// The `RelationFailure` enum must live in `relation_types.rs` and provide
/// structured variant coverage for the semantic families we're unifying.
#[test]
fn test_relation_failure_covers_semantic_families() {
    let source = fs::read_to_string("src/query_boundaries/relation_types.rs")
        .expect("failed to read query_boundaries/relation_types.rs");

    // Core semantic families that must be represented
    for variant in [
        "MissingProperty",
        "MissingProperties",
        "ExcessProperty",
        "IncompatiblePropertyValue",
        "NoApplicableSignature",
        "TupleArityMismatch",
        "ReturnTypeMismatch",
        "ParameterTypeMismatch",
        "ParameterCountMismatch",
        "PropertyModifierMismatch",
        "WeakUnionViolation",
        "TypeMismatch",
    ] {
        assert!(
            source.contains(variant),
            "RelationFailure must include the `{variant}` variant for semantic coverage"
        );
    }
}
