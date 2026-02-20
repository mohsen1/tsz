use crate::flow_analysis::PropertyKey;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

pub(crate) fn constructor_assigned_properties(
    state: &CheckerState<'_>,
    body_idx: NodeIndex,
    tracked: &FxHashSet<PropertyKey>,
    _require_super: bool,
) -> FxHashSet<PropertyKey> {
    state.analyze_constructor_assignments(body_idx, tracked, false)
}

pub(crate) fn check_constructor_property_use_before_assignment(
    state: &mut CheckerState<'_>,
    body_idx: NodeIndex,
    tracked: &FxHashSet<PropertyKey>,
    _require_super: bool,
) {
    state.check_properties_used_before_assigned(body_idx, tracked, false);
}

pub(crate) fn should_report_variable_use_before_assignment(
    state: &mut CheckerState<'_>,
    idx: NodeIndex,
    declared_type: TypeId,
    sym_id: SymbolId,
) -> bool {
    state.should_check_definite_assignment(sym_id, idx)
        && !state.skip_definite_assignment_for_type(declared_type)
        && !state.is_definitely_assigned_at(idx)
}
