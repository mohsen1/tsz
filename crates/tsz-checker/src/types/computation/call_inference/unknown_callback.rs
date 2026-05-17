use crate::call_checker::CallableContext;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{FunctionShape, TypeId};

type CallArgsAndTypes<'a> = (&'a [NodeIndex], &'a [TypeId]);
type ContextualParamTypes<'a> = (Option<&'a [Option<TypeId>]>, &'a [Option<TypeId>]);

impl<'a> CheckerState<'a> {
    pub(crate) fn emit_post_generic_callback_diagnostics(
        &mut self,
        args_and_types: CallArgsAndTypes<'_>,
        contextual_param_types: ContextualParamTypes<'_>,
        shape: Option<&FunctionShape>,
        emit_unknown_callback_body_diagnostics: bool,
        check_excess_properties: bool,
        callable_ctx: CallableContext,
    ) {
        let (args, arg_types) = args_and_types;
        let (finalized_contextual_param_types, base_contextual_param_types) =
            contextual_param_types;
        self.emit_nominal_lib_object_callback_return_errors(
            args,
            arg_types,
            finalized_contextual_param_types,
            base_contextual_param_types,
            shape,
        );
        self.maybe_emit_unknown_callback_body_diagnostics(
            emit_unknown_callback_body_diagnostics,
            shape,
            args,
            arg_types,
            check_excess_properties,
            callable_ctx,
        );
    }

    pub(crate) fn maybe_emit_unknown_callback_body_diagnostics(
        &mut self,
        enabled: bool,
        shape: Option<&FunctionShape>,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        check_excess_properties: bool,
        callable_ctx: CallableContext,
    ) {
        let Some(shape) = shape.filter(|_| enabled) else {
            return;
        };
        self.emit_uninferred_callback_unknown_body_diagnostics(
            shape,
            args,
            arg_types,
            check_excess_properties,
            callable_ctx,
        );
    }

    pub(crate) fn emit_uninferred_callback_unknown_body_diagnostics(
        &mut self,
        shape: &FunctionShape,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        check_excess_properties: bool,
        callable_ctx: CallableContext,
    ) {
        let tracked_type_params: FxHashSet<_> =
            shape.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return;
        }

        for (index, &arg_idx) in args.iter().enumerate() {
            if !self.is_callback_like_argument(arg_idx) {
                continue;
            }
            let Some(param_type) = shape.params.get(index).map(|p| p.type_id).or_else(|| {
                let last = shape.params.last()?;
                last.rest.then_some(last.type_id)
            }) else {
                continue;
            };
            let callback_shape = call_checker::get_contextual_signature(self.ctx.types, param_type)
                .or_else(|| {
                    let evaluated = self.evaluate_type_with_env(param_type);
                    (evaluated != param_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                })
                .or_else(|| {
                    let evaluated = self.evaluate_application_type(param_type);
                    (evaluated != param_type).then(|| {
                        call_checker::get_contextual_signature(self.ctx.types, evaluated)
                    })?
                });
            let Some(callback_shape) = callback_shape else {
                continue;
            };

            let mut substitution = common::TypeSubstitution::new();
            for tp in &shape.type_params {
                let mentioned_in_callback_params = callback_shape.params.iter().any(|param| {
                    common::contains_type_parameter_named(self.ctx.types, param.type_id, tp.name)
                });
                if !mentioned_in_callback_params {
                    continue;
                }

                let has_other_evidence =
                    args.iter()
                        .enumerate()
                        .any(|(other_index, &other_arg_idx)| {
                            self.argument_provides_type_param_evidence(
                                shape,
                                arg_types,
                                index,
                                other_index,
                                other_arg_idx,
                                tp.name,
                            )
                        });
                if has_other_evidence {
                    continue;
                }

                let replacement = tp.default.or(tp.constraint).unwrap_or(TypeId::UNKNOWN);
                let replacement =
                    common::instantiate_type(self.ctx.types, replacement, &substitution);
                let replacement = if common::contains_type_parameters(self.ctx.types, replacement)
                    || common::contains_infer_types(self.ctx.types, replacement)
                {
                    TypeId::UNKNOWN
                } else {
                    replacement
                };
                substitution.insert(tp.name, replacement);
            }

            if substitution.is_empty() {
                continue;
            }

            let contextual_type =
                common::instantiate_type(self.ctx.types, param_type, &substitution);
            if !common::contains_type_by_id(self.ctx.types, contextual_type, TypeId::UNKNOWN) {
                continue;
            }

            let callback_body_spans: Vec<_> = self
                .callback_body_spans(arg_idx)
                .into_iter()
                .filter(|(start, end)| start < end)
                .collect();
            if callback_body_spans.is_empty() {
                continue;
            }

            self.clear_contextual_resolution_cache();
            self.invalidate_expression_for_contextual_retry(arg_idx);
            if let Some(callback_idx) = self.callback_function_index(arg_idx)
                && let Some(callback_node) = self.ctx.arena.get(callback_idx)
                && let Some(func) = self.ctx.arena.get_function(callback_node)
            {
                self.clear_type_cache_recursive(func.body);
            }
            self.compute_callback_argument_type_rollback_unknown_body_diagnostics(
                arg_idx,
                contextual_type,
                check_excess_properties,
                index,
                args.len(),
                callable_ctx,
                &callback_body_spans,
            );
        }
    }

    fn argument_provides_type_param_evidence(
        &self,
        shape: &FunctionShape,
        arg_types: &[TypeId],
        current_index: usize,
        other_index: usize,
        other_arg_idx: NodeIndex,
        type_param_name: tsz_common::Atom,
    ) -> bool {
        if other_index == current_index {
            return false;
        }
        let Some(other_param_type) =
            shape
                .params
                .get(other_index)
                .map(|p| p.type_id)
                .or_else(|| {
                    let last = shape.params.last()?;
                    last.rest.then_some(last.type_id)
                })
        else {
            return false;
        };
        if !common::contains_type_parameter_named(self.ctx.types, other_param_type, type_param_name)
        {
            return false;
        }
        let other_arg_type = arg_types
            .get(other_index)
            .copied()
            .unwrap_or(TypeId::UNKNOWN);
        if other_arg_type == TypeId::ANY
            || other_arg_type == TypeId::UNKNOWN
            || other_arg_type == TypeId::ERROR
            || common::contains_infer_types(self.ctx.types, other_arg_type)
        {
            return false;
        }
        if !self.is_callback_like_argument(other_arg_idx) {
            return true;
        }
        // T appearing in the callback's return type means the callback constrains T.
        if call_checker::get_contextual_signature(self.ctx.types, other_param_type).is_some_and(
            |other_callback| {
                common::contains_type_parameter_named(
                    self.ctx.types,
                    other_callback.return_type,
                    type_param_name,
                )
            },
        ) {
            return true;
        }
        // Fallback for Application types (e.g. `Make<T> = () => T`) whose Lazy base
        // can't be resolved without a TypeEnvironment, so `get_contextual_signature`
        // returns None. If T appears in the Application's args AND the callback's
        // inferred return type is concrete, the callback still constrains T.
        let t_in_application_args = common::application_info(self.ctx.types, other_param_type)
            .is_some_and(|(_, args)| {
                args.iter().any(|&a| {
                    common::contains_type_parameter_named(self.ctx.types, a, type_param_name)
                })
            });
        if !t_in_application_args {
            return false;
        }
        call_checker::get_contextual_signature(self.ctx.types, other_arg_type).is_some_and(|sig| {
            let ret = sig.return_type;
            ret != TypeId::VOID
                && ret != TypeId::UNDEFINED
                && ret != TypeId::NULL
                && !common::contains_type_by_id(self.ctx.types, ret, TypeId::UNKNOWN)
                && !common::contains_type_parameters(self.ctx.types, ret)
                && !common::contains_infer_types(self.ctx.types, ret)
        })
    }
}
