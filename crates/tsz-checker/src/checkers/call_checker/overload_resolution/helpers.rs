//! Small standalone helpers for overload resolution — pure code motion from
//! the parent `overload_resolution` module.

use crate::query_boundaries::checkers::call::{
    array_element_type_for_type, get_contextual_signature,
};
use crate::query_boundaries::common::{CallResult, ContextualTypeContext};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use super::super::OverloadResolution;
use super::retry_state::NoReturnContextFallback;

impl<'a> CheckerState<'a> {
    /// Report whether the union-of-signatures contextual type used by the
    /// first overload pass is lossy for some callback argument.
    ///
    /// tsc's `chooseOverload` types callback arguments against each candidate
    /// signature individually. Pass 1 instead types every argument once under
    /// a union of all overload signatures; per tsc's `getIntersectedSignatures`
    /// rule a union mixing generic and non-generic (or differently-shaped)
    /// call signatures yields NO contextual signature, so the callback's own
    /// parameters silently degrade to implicit `any`. Inference computed from
    /// such a callback body (e.g. a nested generic call falling back to
    /// `unknown`) is unreliable and must not decide overload selection. When
    /// this returns `true`, pass-1 successes are deferred to the
    /// signature-specific second pass, which retypes callbacks per candidate
    /// exactly like tsc.
    pub(super) fn union_context_lossy_for_callback_args(
        &self,
        args: &[NodeIndex],
        union_contextual_param_types: &[Option<TypeId>],
        signature_types: &[TypeId],
    ) -> bool {
        if signature_types.len() < 2 {
            return false;
        }
        args.iter().enumerate().any(|(i, &arg_idx)| {
            if !self.is_callback_like_argument(arg_idx) {
                return false;
            }
            let union_provides_signature = union_contextual_param_types
                .get(i)
                .copied()
                .flatten()
                .is_some_and(|param_type| {
                    get_contextual_signature(self.ctx.types, param_type).is_some()
                });
            if union_provides_signature {
                return false;
            }
            signature_types.iter().any(|&sig_type| {
                ContextualTypeContext::with_expected_and_options(
                    self.ctx.types,
                    sig_type,
                    self.ctx.compiler_options.no_implicit_any,
                )
                .get_parameter_type_for_call(i, args.len())
                .is_some_and(|param_type| {
                    get_contextual_signature(self.ctx.types, param_type).is_some()
                })
            })
        })
    }

    /// Accept a success deferred by the first overload pass: restore its
    /// snapshot, merge the union-pass argument node types, and build the
    /// resolution.
    pub(super) fn accept_first_pass_success_fallback(
        &mut self,
        fallback: NoReturnContextFallback,
        temp_node_types: &crate::context::NodeTypeCache,
    ) -> OverloadResolution {
        let (arg_types, return_type, selected_type_predicate, snap) = fallback;
        self.ctx.rollback_full(&snap);
        self.ctx.node_types.merge(temp_node_types);
        OverloadResolution {
            arg_types,
            result: CallResult::Success(return_type),
            selected_type_predicate,
        }
    }

    pub(super) fn signature_const_type_params_require_readonly_argument_context(
        db: &dyn tsz_solver::construction::TypeDatabase,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> bool {
        type_params.iter().any(|type_param| {
            type_param.is_const
                && !type_param.constraint.is_some_and(|constraint| {
                    Self::constraint_allows_mutable_array_like(db, constraint)
                })
        })
    }

    pub(super) fn overload_string_argument_array_parameter_mismatch(
        &mut self,
        sig: &tsz_solver::CallSignature,
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        // Position of the trailing rest parameter, if any. Arguments at or past
        // this index are matched element-wise against the rest's element type,
        // not against the rest's (array-like) declared type — so a `string`
        // spread element landing on a `...items: string[]` rest is *not* a
        // mismatch. Without this, a non-tuple spread (`f(..., ...stringArr)`)
        // whose element type was collected as `string` would be wrongly rejected
        // here as "string argument vs array parameter".
        let rest_start = sig
            .params
            .last()
            .filter(|param| param.rest)
            .map(|_| sig.params.len() - 1);
        arg_types
            .iter()
            .copied()
            .enumerate()
            .find_map(|(index, actual)| {
                if actual != TypeId::STRING
                    && !crate::query_boundaries::common::is_string_type(self.ctx.types, actual)
                    && crate::query_boundaries::common::string_literal_value(self.ctx.types, actual)
                        .is_none()
                {
                    return None;
                }
                let expected = if rest_start.is_some_and(|start| index >= start) {
                    // `index` lands on the trailing rest parameter: compare
                    // against its element type.
                    let rest_type = sig.params.last()?.type_id;
                    array_element_type_for_type(self.ctx.types, rest_type).unwrap_or(rest_type)
                } else {
                    sig.params.get(index).map(|param| param.type_id)?
                };
                self.is_array_like_type(expected)
                    .then_some(CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: sig.return_type,
                    })
            })
    }
}
