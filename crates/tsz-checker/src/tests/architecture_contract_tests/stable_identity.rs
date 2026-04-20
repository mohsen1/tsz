use super::*;
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
