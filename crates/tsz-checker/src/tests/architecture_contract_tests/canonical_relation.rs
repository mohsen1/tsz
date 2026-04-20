use super::*;
// Canonical RelationRequest / RelationOutcome boundary tests
// ========================================================================
//
// These tests enforce that the canonical `RelationRequest` / `RelationOutcome`
// / `execute_relation` boundary is the single authoritative path for relation
// queries that need structured failure information.

/// The `query_boundaries/assignability.rs` boundary must expose the unified
/// `execute_relation` helper and the `RelationOutcome` / `RelationRequest`
/// types that the checker uses for single-pass relation + failure collection.
#[test]
fn test_relation_request_and_outcome_live_in_query_boundaries() {
    let boundary_source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        boundary_source.contains("pub(crate) struct RelationRequest"),
        "RelationRequest must be defined in query_boundaries/assignability.rs"
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
        boundary_source.contains("pub kind: RelationKind"),
        "RelationRequest must include a RelationKind field"
    );
    assert!(
        boundary_source.contains("pub excess_property_mode: ExcessPropertyMode"),
        "RelationRequest must include an ExcessPropertyMode field"
    );
    assert!(
        boundary_source.contains("pub missing_property_mode: MissingPropertyMode"),
        "RelationRequest must include a MissingPropertyMode field"
    );
    assert!(
        boundary_source.contains("pub source_is_fresh: bool"),
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
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    for variant in [
        "Assign",
        "CallArg",
        "Return",
        "JsxProps",
        "Destructuring",
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
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

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
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

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
    ] {
        assert!(
            source.contains(flag),
            "RelationFlags must expose the `{flag}` constant"
        );
    }
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
