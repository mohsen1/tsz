/// Guard: `ensure_def_ready_for_lowering` delegates to
/// `extract_declared_type_params_for_reference_symbol` (not inline loops).
#[test]
fn test_ensure_def_ready_delegates_to_extract_declared_params() {
    let src = fs::read_to_string("src/state/type_resolution/reference_helpers.rs")
        .expect("failed to read src/state/type_resolution/reference_helpers.rs");

    // Find the ensure_def_ready_for_lowering body and check it calls
    // extract_declared_type_params_for_reference_symbol
    let in_helper = src
        .lines()
        .skip_while(|line| !line.contains("fn ensure_def_ready_for_lowering"))
        .take(30)
        .any(|line| line.contains("extract_declared_type_params_for_reference_symbol"));

    assert!(
        in_helper,
        "ensure_def_ready_for_lowering must delegate to \
         extract_declared_type_params_for_reference_symbol for type-param \
         extraction — no inline declaration iteration."
    );
}

/// Guard: `namespace_checker.rs` must NOT directly construct `TypeData::Lazy`
/// outside of documented exceptions for pure-namespace member handling.
///
/// Namespace types should use structural object types (via `build_namespace_object_type`)
/// or stable-identity helpers — except for pure-namespace sub-members which require
/// Lazy(DefId) to avoid infinite recursion during subtype checks.
#[test]
fn test_namespace_checker_no_raw_lazy_construction() {
    let src = fs::read_to_string("src/declarations/namespace_checker.rs")
        .expect("failed to read src/declarations/namespace_checker.rs");

    // Count occurrences of .lazy( outside comments
    let lazy_count = src
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .filter(|line| line.contains(".lazy("))
        .count();

    // Currently 2 allowed usages for pure-namespace members:
    // 1. get_type_of_class_namespace_member (line ~264)
    // 2. build_namespace_object_type for is_pure_namespace (line ~774)
    const ALLOWED_LAZY_COUNT: usize = 2;

    assert!(
        lazy_count <= ALLOWED_LAZY_COUNT,
        "namespace_checker.rs has {lazy_count} .lazy() calls (allowed: {ALLOWED_LAZY_COUNT}). \
         Namespace types should use structural object types \
         (build_namespace_object_type) or stable-identity helpers. \
        Only pure-namespace sub-members may use Lazy(DefId) to avoid recursion."
    );
}

/// Guard: diagnostic-bearing assignability paths should use named
/// `RelationOutcome` helpers instead of locally constructing relation requests.
#[test]
fn test_assignability_diagnostics_route_through_relation_outcome_helpers() {
    let relation_src = fs::read_to_string("src/assignability/assignability_relation.rs")
        .expect("failed to read src/assignability/assignability_relation.rs");
    for helper in [
        "fn assign_relation_outcome",
        "fn call_arg_relation_outcome",
        "fn bivariant_callbacks_relation_outcome",
    ] {
        assert!(
            relation_src.contains(helper),
            "assignability_relation.rs must expose {helper} for diagnostic relation decisions"
        );
    }

    let diagnostic_files = [
        "src/assignability/assignability_diagnostics.rs",
        "src/assignability/assignment_checker/destructuring.rs",
    ];
    let forbidden = [
        "RelationRequest::assign(",
        "RelationRequest::call_arg(",
        "RelationRequest::bivariant_callbacks(",
    ];

    let mut violations = Vec::new();
    for path in diagnostic_files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("failed to read {path} for architecture guard"));
        for pattern in forbidden {
            if source.contains(pattern) {
                violations.push(format!("{path} contains {pattern}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "diagnostic assignability paths should call named RelationOutcome helpers; violations:\n{}",
        violations.join("\n")
    );
}
