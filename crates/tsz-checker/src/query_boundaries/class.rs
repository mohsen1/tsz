use crate::class_checker::ClassMemberInfo;
use crate::state::CheckerState;
use tsz_parser::NodeIndex;
use tsz_solver::TypeId;

// =============================================================================
// Relation boundary helpers (thin wrappers over assignability)
// =============================================================================

/// Check if a member type mismatch should be reported (TS2416).
///
/// Uses `no_erase_generics` mode to match tsc's `compareSignaturesRelated`
/// behavior for implements/extends member checking: a non-generic function
/// like `(x: string) => string` is NOT assignable to a generic function
/// like `<T>(x: T) => T`, ensuring TS2416 is correctly emitted.
pub(crate) fn should_report_member_type_mismatch(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    let source = checker.narrow_this_from_enclosing_typeof_guard(node_idx, source);
    if checker.should_suppress_assignability_diagnostic(source, target) {
        return false;
    }
    if checker.should_suppress_assignability_for_parse_recovery(node_idx, node_idx) {
        return false;
    }
    !checker.is_assignable_to_no_erase_generics(source, target)
        && !checker.should_skip_weak_union_error(source, target, node_idx)
}

pub(crate) fn should_report_member_type_mismatch_bivariant(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    checker.should_report_assignability_mismatch_bivariant(source, target, node_idx)
}

// =============================================================================
// OwnMemberSummary — single-pass class member extraction
// =============================================================================

/// Summary of a single class's own members, extracted in one pass.
///
/// Contains ALL members (including private). Consumers filter by visibility
/// as needed. Only instance and static member vectors are populated; other
/// derived views (display names, kinds, parameter properties) were removed
/// as they had no callers.
#[derive(Clone, Default)]
pub(crate) struct OwnMemberSummary {
    /// All instance members (including private).
    pub(crate) all_instance_members: Vec<ClassMemberInfo>,
    /// All static members (including private).
    pub(crate) all_static_members: Vec<ClassMemberInfo>,
}

// =============================================================================
// Construction boundary function
// =============================================================================

/// Build the own-member summary for a class via single-pass extraction.
///
/// Extracts each member once (with `skip_private=false`) and records it
/// into the instance or static member vector.
pub(crate) fn build_own_member_summary(
    checker: &mut CheckerState<'_>,
    class_data: &tsz_parser::parser::node::ClassData,
) -> OwnMemberSummary {
    let mut summary = OwnMemberSummary::default();

    for &member_idx in &class_data.members.nodes {
        // Extract member info once (skip_private=false -> all members)
        if let Some(info) = checker.extract_class_member_info(member_idx, false) {
            if info.is_static {
                summary.all_static_members.push(info);
            } else {
                summary.all_instance_members.push(info);
            }
        }
    }

    summary
}
