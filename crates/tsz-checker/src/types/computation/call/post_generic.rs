use crate::call_checker::CallableContext;
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{FunctionShape, TypeId};

pub(super) struct PostGenericCallDiagnostics<'a> {
    pub(super) result: &'a mut CallResult,
    pub(super) allow_contextual_mismatch_deferral: &'a mut bool,
    pub(super) callee_type_for_call: TypeId,
    pub(super) args: &'a [NodeIndex],
    pub(super) arg_types: &'a [TypeId],
    pub(super) base_contextual_param_types: &'a [Option<TypeId>],
    pub(super) finalized_contextual_param_types: Option<&'a [Option<TypeId>]>,
    pub(super) original_callee_shape: Option<&'a FunctionShape>,
    pub(super) emit_unknown_callback_body_diagnostics: bool,
    pub(super) check_excess_properties: bool,
    pub(super) callable_ctx: CallableContext,
}

impl<'a> CheckerState<'a> {
    pub(super) fn run_post_generic_call_diagnostics(
        &mut self,
        diagnostics: PostGenericCallDiagnostics<'_>,
    ) {
        self.repair_abstract_constructor_argument_mismatch(
            diagnostics.result,
            diagnostics.allow_contextual_mismatch_deferral,
            diagnostics.callee_type_for_call,
            diagnostics.args,
            diagnostics.arg_types,
            diagnostics.base_contextual_param_types,
            diagnostics.finalized_contextual_param_types,
        );
        self.emit_post_generic_callback_diagnostics(
            (diagnostics.args, diagnostics.arg_types),
            (
                diagnostics.finalized_contextual_param_types,
                diagnostics.base_contextual_param_types,
            ),
            diagnostics.original_callee_shape,
            diagnostics.emit_unknown_callback_body_diagnostics,
            diagnostics.check_excess_properties,
            diagnostics.callable_ctx,
        );
    }
}
