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
            let Some(callback_shape) = self.resolved_callback_contextual_signature(param_type)
            else {
                continue;
            };

            let mut substitution =
                crate::query_boundaries::generic_instantiation::signature_domain_substitution(
                    &shape.type_params,
                );
            for tp in &shape.type_params {
                let mentioned_in_callback_params = callback_shape.params.iter().any(|param| {
                    crate::query_boundaries::generic_instantiation::type_contains_type_parameter_binder(
                        self.ctx.types,
                        param.type_id,
                        *tp,
                    )
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
                                *tp,
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
        &mut self,
        shape: &FunctionShape,
        arg_types: &[TypeId],
        current_index: usize,
        other_index: usize,
        other_arg_idx: NodeIndex,
        type_param: tsz_solver::TypeParamInfo,
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
        if !crate::query_boundaries::generic_instantiation::type_contains_type_parameter_binder(
            self.ctx.types,
            other_param_type,
            type_param,
        ) {
            return false;
        }
        let other_arg_type = arg_types
            .get(other_index)
            .copied()
            .unwrap_or(TypeId::UNKNOWN);
        // `any` is a fully resolved, deliberate type: `tsc` infers the shared
        // type parameter as `any` from an `any`-typed sibling argument (the
        // same candidate Round 1 inference already computes), so it counts as
        // evidence like any other concrete type. Only `unknown`/`error`/still
        // resolving (`infer`-containing) sibling types are genuinely
        // uninformative and must not gate off this diagnostic.
        if other_arg_type == TypeId::UNKNOWN
            || other_arg_type == TypeId::ERROR
            || common::contains_infer_types(self.ctx.types, other_arg_type)
        {
            return false;
        }
        if !self.is_callback_like_argument(other_arg_idx) {
            return true;
        }
        let Some(other_callback) = self.resolved_callback_contextual_signature(other_param_type)
        else {
            return false;
        };
        // The return position of the sibling callback's contextual signature
        // always counts as inference evidence.
        if crate::query_boundaries::generic_instantiation::type_contains_type_parameter_binder(
            self.ctx.types,
            other_callback.return_type,
            type_param,
        ) {
            return true;
        }
        // A parameter position counts only when the sibling lambda annotates
        // that parameter itself. An *unannotated* parameter is an inference
        // sink: its type is produced by the contextual type, so it supplies
        // context TO the lambda from `T` rather than inference FROM the lambda
        // back to `T`. An *annotated* parameter is the opposite — `tsc` infers
        // the enclosing signature's type parameters contravariantly from the
        // annotation and fixes them before contextually typing any sibling
        // callback body. The position matters: an annotation sitting at a slot
        // whose contextual type does not mention `T` is not evidence for `T`.
        let annotated_positions = self.annotated_callback_parameter_positions(other_arg_idx);
        let param_type_at = |position: usize| -> Option<TypeId> {
            other_callback
                .params
                .get(position)
                .map(|param| param.type_id)
                .or_else(|| {
                    let last = other_callback.params.last()?;
                    last.rest.then_some(last.type_id)
                })
        };
        annotated_positions.iter().any(|&position| {
            param_type_at(position).is_some_and(|param_type| {
                crate::query_boundaries::generic_instantiation::type_contains_type_parameter_binder(
                    self.ctx.types,
                    param_type,
                    type_param,
                )
            })
        })
    }

    /// Positions of a callback argument's own parameters that carry an
    /// explicit type annotation, and are therefore inference sources rather
    /// than sinks for the enclosing generic signature's type parameters.
    fn annotated_callback_parameter_positions(&self, arg_idx: NodeIndex) -> Vec<usize> {
        let Some(callback_idx) = self.callback_function_index(arg_idx) else {
            return Vec::new();
        };
        let Some(func) = self
            .ctx
            .arena
            .get(callback_idx)
            .and_then(|node| self.ctx.arena.get_function(node))
        else {
            return Vec::new();
        };
        func.parameters
            .nodes
            .iter()
            .enumerate()
            .filter(|&(_, &param_idx)| {
                self.ctx
                    .arena
                    .get(param_idx)
                    .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                    .is_some_and(|param| param.type_annotation.is_some())
            })
            .map(|(position, _)| position)
            .collect()
    }

    /// Resolve a callback parameter slot to its contextual `FunctionShape`,
    /// falling through `evaluate_type_with_env` and then
    /// `evaluate_application_type` so aliased callable interfaces (e.g.
    /// `Make<T>`, stored as `Application`) are not seen as opaque.
    fn resolved_callback_contextual_signature(&mut self, ty: TypeId) -> Option<FunctionShape> {
        if let Some(shape) = call_checker::get_contextual_signature(self.ctx.types, ty) {
            return Some(shape);
        }
        let evaluated = self.evaluate_type_with_env(ty);
        if evaluated != ty
            && let Some(shape) = call_checker::get_contextual_signature(self.ctx.types, evaluated)
        {
            return Some(shape);
        }
        let evaluated = self.evaluate_application_type(ty);
        if evaluated != ty {
            return call_checker::get_contextual_signature(self.ctx.types, evaluated);
        }
        None
    }
}
