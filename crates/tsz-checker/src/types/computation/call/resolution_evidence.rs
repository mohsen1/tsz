use crate::call_checker::CheckerCallResolution;
use crate::state::CheckerState;
use tsz_solver::TypeId;

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_callable_with_evidence(
    state: &mut CheckerState<'_>,
    is_super_call: bool,
    callee_type: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
    actual_this_type: Option<TypeId>,
    arg_source_markers: &[bool],
) -> CheckerCallResolution {
    if is_super_call {
        return CheckerCallResolution {
            result: state.resolve_new_with_checker_adapter(
                callee_type,
                arg_types,
                force_bivariant_callbacks,
                contextual_type,
            ),
            selected_type_predicate: None,
            instantiated_params: None,
            relation_evidence: Vec::new(),
        };
    }
    if arg_source_markers.iter().any(|&marker| marker) {
        state.resolve_call_with_checker_adapter_and_arg_sources_evidence(
            callee_type,
            arg_types,
            force_bivariant_callbacks,
            contextual_type,
            actual_this_type,
            arg_source_markers,
        )
    } else {
        state.resolve_call_with_checker_adapter_evidence(
            callee_type,
            arg_types,
            force_bivariant_callbacks,
            contextual_type,
            actual_this_type,
        )
    }
}
