//! Callback display helpers for call error diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ParameterData;
use tsz_solver::{FunctionShape, TypeId};

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter::call_errors) fn contextual_function_argument_parameter_display_type(
        &mut self,
        arg_idx: NodeIndex,
        expected: TypeId,
        index: usize,
        param: &ParameterData,
        shape: &FunctionShape,
    ) -> TypeId {
        if !self.target_can_contextually_type_callback_params(arg_idx, expected)
            && param.type_annotation.is_none()
        {
            return TypeId::ANY;
        }

        self.contextual_parameter_type_with_env_from_expected(
            expected,
            index,
            param.dot_dot_dot_token,
        )
        .or_else(|| shape.params.get(index).map(|param| param.type_id))
        .unwrap_or(TypeId::ANY)
    }
}
