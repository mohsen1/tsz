//! Narrow callback return diagnostics for nominal lib object constraints.

use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common;
use crate::state::CheckerState;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{FunctionShape, TypeId};

impl<'a> CheckerState<'a> {
    pub(crate) fn emit_nominal_lib_object_callback_return_errors(
        &mut self,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        finalized_contextual_param_types: Option<&[Option<TypeId>]>,
        base_contextual_param_types: &[Option<TypeId>],
        callee_shape: Option<&FunctionShape>,
    ) {
        for (i, &arg_idx) in args.iter().enumerate() {
            let Some(return_expr) = self.unannotated_zero_param_callback_return_expression(arg_idx)
            else {
                continue;
            };
            let Some(body_node) = self.ctx.arena.get(return_expr) else {
                continue;
            };
            if body_node.kind == syntax_kind_ext::BLOCK
                || self.has_diagnostic_code_within_span(
                    body_node.pos,
                    body_node.end,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            {
                continue;
            }

            let mut expected = finalized_contextual_param_types
                .and_then(|types| types.get(i).copied().flatten())
                .and_then(|ty| self.nominal_lib_object_callback_return_type(ty))
                .or_else(|| {
                    base_contextual_param_types
                        .get(i)
                        .copied()
                        .flatten()
                        .and_then(|ty| self.nominal_lib_object_callback_return_type(ty))
                });

            if expected.is_none()
                && let Some(shape) = callee_shape
                && let Some(param_type) = shape.params.get(i).map(|p| p.type_id).or_else(|| {
                    let last = shape.params.last()?;
                    last.rest.then_some(last.type_id)
                })
            {
                expected = self.nominal_lib_object_callback_return_type(param_type);
            }

            let Some(expected) = expected else {
                continue;
            };

            let Some(actual) = arg_types
                .get(i)
                .copied()
                .and_then(|ty| self.callback_return_type_from_type(ty))
            else {
                continue;
            };
            if matches!(actual, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
                continue;
            }

            let actual_for_check = common::widen_type(self.ctx.types, actual);
            if !matches!(
                actual_for_check,
                TypeId::STRING
                    | TypeId::NUMBER
                    | TypeId::BOOLEAN
                    | TypeId::BIGINT
                    | TypeId::SYMBOL
                    | TypeId::NULL
                    | TypeId::UNDEFINED
            ) && !common::is_primitive_type(self.ctx.types, actual_for_check)
            {
                continue;
            }
            if self.is_assignable_to_with_env(actual_for_check, expected) {
                continue;
            }

            let source_display = self.format_type_diagnostic(actual_for_check);
            let target_display = self.format_type_diagnostic(expected);
            let message = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_display, &target_display],
            );
            self.ctx.error(
                body_node.pos,
                body_node.end.saturating_sub(body_node.pos),
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
    }

    pub(crate) fn nominal_lib_object_callback_return_type(
        &mut self,
        callback_type: TypeId,
    ) -> Option<TypeId> {
        let callback_shape = call_checker::get_contextual_signature(self.ctx.types, callback_type)
            .or_else(|| {
                common::function_shape_for_type(self.ctx.types, callback_type).map(|s| {
                    let shape = (*s).clone();
                    FunctionShape {
                        type_params: shape.type_params,
                        params: shape.params,
                        this_type: shape.this_type,
                        return_type: shape.return_type,
                        type_predicate: shape.type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    }
                })
            })?;
        self.nominal_lib_object_type(callback_shape.return_type)
    }

    fn callback_return_type_from_type(&mut self, callback_type: TypeId) -> Option<TypeId> {
        call_checker::get_contextual_signature(self.ctx.types, callback_type)
            .map(|shape| shape.return_type)
            .or_else(|| {
                common::function_shape_for_type(self.ctx.types, callback_type)
                    .map(|shape| shape.return_type)
            })
    }

    fn nominal_lib_object_type(&mut self, ty: TypeId) -> Option<TypeId> {
        let ty = if let Some(info) = common::type_param_info(self.ctx.types, ty) {
            info.constraint?
        } else {
            ty
        };
        let name = self.format_type_diagnostic(ty);
        if !self.is_nominal_lib_object_type_name(&name) {
            return None;
        }
        let evaluated = self.evaluate_type_with_env(ty);
        let expected = if evaluated == TypeId::ANY && ty != TypeId::ANY {
            ty
        } else {
            evaluated
        };
        (!matches!(expected, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)).then_some(expected)
    }
}
