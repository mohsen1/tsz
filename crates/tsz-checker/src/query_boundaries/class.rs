use crate::state::CheckerState;
use tsz_parser::NodeIndex;
use tsz_solver::TypeId;

pub(crate) fn should_report_member_type_mismatch(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    checker.should_report_assignability_mismatch(source, target, node_idx)
}
