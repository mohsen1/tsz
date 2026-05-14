use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn repair_abstract_constructor_argument_mismatch(
        &mut self,
        result: &mut CallResult,
        allow_contextual_mismatch_deferral: &mut bool,
        _callee_type_for_call: TypeId,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        base_contextual_param_types: &[Option<TypeId>],
        finalized_contextual_param_types: Option<&[Option<TypeId>]>,
    ) {
        let CallResult::Success(return_type) = *result else {
            return;
        };

        for (index, (&_arg_idx, &actual)) in args.iter().zip(arg_types.iter()).enumerate() {
            let expected = base_contextual_param_types
                .get(index)
                .copied()
                .flatten()
                .or_else(|| {
                    finalized_contextual_param_types
                        .and_then(|types| types.get(index).copied().flatten())
                });
            let Some(expected) = expected else {
                continue;
            };
            if self.constructor_abstractness_for_assignability(actual) == Some(true)
                && self.constructor_abstractness_for_assignability(expected) == Some(false)
            {
                *allow_contextual_mismatch_deferral = false;
                *result = CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    fallback_return: return_type,
                };
                break;
            }
        }
    }
}
