use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{FunctionShape, TypeData, TypeId};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn type_param_name_if_generic_rest_tuple_param(
        &self,
        func: &FunctionShape,
        type_id: TypeId,
    ) -> Option<tsz_common::Atom> {
        let type_id = self.unwrap_readonly(type_id);
        let Some(TypeData::TypeParameter(info)) = self.interner.lookup(type_id) else {
            return None;
        };

        func.type_params
            .iter()
            .any(|type_param| type_param.name == info.name)
            .then_some(info.name)
    }

    /// Mirror tsc's implied-arity assignment for a non-array rest type parameter.
    ///
    /// When a signature ends in `...rest: T` where `T` is a bare type parameter,
    /// the trailing arguments that fall into the rest parameter fix `T`'s arity.
    /// Variadic tuple inference reads this to split adjacent variadic elements of
    /// a `[...A, ...B]` target. Skips (leaves the arity unset) when a spread
    /// argument appears among the fixed parameters, matching tsc.
    pub(super) fn record_rest_param_implied_arity(
        &mut self,
        infer_ctx: &mut InferenceContext,
        func: &FunctionShape,
        arg_types: &[TypeId],
        type_param_vars: &[InferenceVar],
    ) {
        let Some(rest_param) = func.params.last().filter(|param| param.rest) else {
            return;
        };
        let Some(rest_name) =
            self.type_param_name_if_generic_rest_tuple_param(func, rest_param.type_id)
        else {
            return;
        };
        let Some(var) = func
            .type_params
            .iter()
            .zip(type_param_vars.iter())
            .find_map(|(tp, &var)| (tp.name == rest_name).then_some(var))
        else {
            return;
        };

        let fixed_param_count = func.params.len().saturating_sub(1);
        let arg_count = fixed_param_count.min(arg_types.len());
        let spread_in_fixed = arg_types[..arg_count]
            .iter()
            .any(|&arg| self.spread_argument_marker_inner(arg).is_some());
        if spread_in_fixed {
            return;
        }

        infer_ctx.set_implied_arity(var, arg_types.len().saturating_sub(arg_count));
    }
}
