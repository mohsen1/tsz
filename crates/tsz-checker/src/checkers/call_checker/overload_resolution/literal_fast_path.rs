use crate::query_boundaries::checkers::call::{get_contextual_signature, lazy_def_id_for_type};
use crate::query_boundaries::common::{
    CallResult, FunctionShape, is_string_type, string_literal_value,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::super::{CallableContext, OverloadResolution};

impl<'a> CheckerState<'a> {
    fn overload_fast_path_literal_arg(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        self.ctx.arena.kind_at(idx).is_some_and(|kind| {
            matches!(
                kind,
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::BigIntLiteral as u16
                    || k == SyntaxKind::TrueKeyword as u16
                    || k == SyntaxKind::FalseKeyword as u16
                    || k == SyntaxKind::NullKeyword as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            )
        })
    }

    pub(super) fn try_resolve_literal_overloaded_call_fast_path(
        &mut self,
        args: &[NodeIndex],
        signatures: &[tsz_solver::CallSignature],
        force_bivariant_callbacks: bool,
    ) -> Option<OverloadResolution> {
        if !args
            .iter()
            .copied()
            .all(|arg| self.overload_fast_path_literal_arg(arg))
        {
            return None;
        }

        let prev_preserve_literals = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |_i, _arg_count| None,
            false,
            None,
            CallableContext::none(),
        );
        self.ctx.preserve_literal_types = prev_preserve_literals;

        let factory = self.ctx.types.factory();
        for (idx, original_sig) in signatures.iter().enumerate() {
            let sig = self.overload_signature_for_inference(original_sig, idx, &arg_types, None);
            if self.call_signature_has_obvious_literal_arg_mismatch(&sig, args) {
                continue;
            }
            let sig_shape = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: sig.is_method,
            };
            let func_type = factory.function(sig_shape);
            self.ensure_relation_input_ready(func_type);
            let resolved_func_type =
                if let Some(def_id) = lazy_def_id_for_type(self.ctx.types, func_type) {
                    self.ctx
                        .type_env
                        .borrow()
                        .get_def(def_id)
                        .unwrap_or(func_type)
                } else {
                    func_type
                };
            let (mut result, instantiated_predicate, _) = if let Some(result) =
                self.overload_string_argument_array_parameter_mismatch(&sig, &arg_types)
            {
                (result, None, None)
            } else {
                self.resolve_call_with_checker_adapter(
                    resolved_func_type,
                    &arg_types,
                    force_bivariant_callbacks,
                    None,
                    None,
                )
            };
            if let CallResult::ArgumentTypeMismatch {
                expected,
                actual,
                fallback_return,
                ..
            } = result
                && self.type_is_or_constrained_to_top_rest_any_callable(expected)
                && get_contextual_signature(self.ctx.types, actual).is_some()
            {
                result = CallResult::Success(fallback_return);
            }
            if let CallResult::Success(_) = result {
                let selected_type_predicate =
                    Self::selected_overload_type_predicate(&sig, instantiated_predicate);
                return Some(OverloadResolution {
                    arg_types,
                    result,
                    selected_type_predicate,
                });
            }
        }

        None
    }

    pub(super) fn overload_string_argument_array_parameter_mismatch(
        &mut self,
        sig: &tsz_solver::CallSignature,
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        arg_types
            .iter()
            .copied()
            .enumerate()
            .find_map(|(index, actual)| {
                if actual != TypeId::STRING
                    && !is_string_type(self.ctx.types, actual)
                    && string_literal_value(self.ctx.types, actual).is_none()
                {
                    return None;
                }
                let expected = sig
                    .params
                    .get(index)
                    .map(|param| param.type_id)
                    .or_else(|| {
                        sig.params
                            .last()
                            .and_then(|param| param.rest.then_some(param.type_id))
                    })?;
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
