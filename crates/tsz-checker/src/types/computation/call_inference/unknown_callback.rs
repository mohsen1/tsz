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
            finalized_contextual_param_types,
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
        finalized_contextual_param_types: Option<&[Option<TypeId>]>,
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
            finalized_contextual_param_types,
            check_excess_properties,
            callable_ctx,
        );
    }

    pub(crate) fn emit_uninferred_callback_unknown_body_diagnostics(
        &mut self,
        shape: &FunctionShape,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        finalized_contextual_param_types: Option<&[Option<TypeId>]>,
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

                let has_other_evidence = self
                    .bare_type_param_position_resolved_by_round1(
                        shape,
                        finalized_contextual_param_types,
                        index,
                        *tp,
                    )
                    .is_some()
                    || args
                        .iter()
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
        if !self.type_or_predicate_target_mentions_type_param(other_param_type, type_param) {
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
        // always counts as inference evidence — including a type-predicate
        // target (`x is T`), which is the sibling's only inference channel
        // for callbacks like `isApplicable: (v: any) => v is I`.
        if crate::query_boundaries::generic_instantiation::type_contains_type_parameter_binder(
            self.ctx.types,
            other_callback.return_type,
            type_param,
        ) || other_callback
            .type_predicate
            .and_then(|predicate| predicate.type_id)
            .is_some_and(|predicate_type| {
                crate::query_boundaries::generic_instantiation::type_contains_type_parameter_binder(
                    self.ctx.types,
                    predicate_type,
                    type_param,
                )
            })
        {
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

    /// Whether the call's own already-computed generic solve (threaded down
    /// as `finalized_contextual_param_types`, the callee's parameter types
    /// after substituting the real inferred type arguments) resolved
    /// `type_param` to a concrete type through some OTHER bare-typed
    /// parameter position.
    ///
    /// This complements `argument_provides_type_param_evidence`'s
    /// argument-shape heuristic rather than replacing it (#16018): for a
    /// parameter slot declared as a bare type-parameter reference (`x: T`,
    /// no nesting), substituting the solver's real answer into that slot
    /// yields `type_param`'s own resolved value directly, with no need to
    /// re-derive "was there evidence" from the sibling argument's shape.
    /// That answer can be more informed than the raw per-argument
    /// `arg_types` the heuristic reads — e.g. a sibling argument that itself
    /// failed to check cleanly still leaves the OTHER, correctly computed
    /// parameter substitution behind. A parameter slot declared as a
    /// compound type mentioning `type_param` (`Array<T>`, a generic alias or
    /// wrapper) is handled the same way, structurally: `type_param` cannot
    /// be read off the slot by identity, so it is recovered by matching the
    /// declared type against the finalized concrete type through the same
    /// structural-unification primitive `predicate_resolution.rs` already
    /// uses to instantiate a nested type-predicate target.
    fn bare_type_param_position_resolved_by_round1(
        &self,
        shape: &FunctionShape,
        finalized_contextual_param_types: Option<&[Option<TypeId>]>,
        current_index: usize,
        type_param: tsz_solver::TypeParamInfo,
    ) -> Option<TypeId> {
        let finalized = finalized_contextual_param_types?;
        shape
            .params
            .iter()
            .enumerate()
            .find_map(|(position, param)| {
                if position == current_index {
                    return None;
                }
                let resolved = finalized.get(position).copied().flatten()?;
                if resolved == TypeId::UNKNOWN
                    || resolved == TypeId::ERROR
                    || common::contains_infer_types(self.ctx.types, resolved)
                    || common::contains_type_parameters(self.ctx.types, resolved)
                {
                    return None;
                }
                if let Some(bare_type_param) =
                    common::type_param_info(self.ctx.types, param.type_id)
                {
                    return bare_type_param
                        .is_same_binder(type_param)
                        .then_some(resolved);
                }
                self.nested_type_param_resolved_by_structural_match(
                    param.type_id,
                    resolved,
                    type_param,
                )
            })
    }

    /// Recover `type_param`'s binding from a compound declared parameter
    /// type (e.g. `Array<T>`) by structurally matching it against the
    /// corresponding finalized concrete type (e.g. `Array<string>`) Round 1
    /// already computed. Returns `None` when `declared` does not mention
    /// `type_param` at all, or when the structural match yields nothing
    /// usable (still a type parameter, `unknown`, `error`, or unresolved
    /// `infer`).
    fn nested_type_param_resolved_by_structural_match(
        &self,
        declared: TypeId,
        resolved: TypeId,
        type_param: tsz_solver::TypeParamInfo,
    ) -> Option<TypeId> {
        if !crate::query_boundaries::generic_instantiation::type_contains_type_parameter_binder(
            self.ctx.types,
            declared,
            type_param,
        ) {
            return None;
        }
        let bindings =
            crate::query_boundaries::generic_instantiation::infer_type_arguments_from_param_args(
                self.ctx.types,
                std::slice::from_ref(&type_param),
                &[(declared, resolved)],
            );
        bindings.into_iter().find_map(|(name, inferred)| {
            (name == type_param.name
                && inferred != TypeId::UNKNOWN
                && inferred != TypeId::ERROR
                && !common::contains_infer_types(self.ctx.types, inferred)
                && !common::contains_type_parameters(self.ctx.types, inferred))
            .then_some(inferred)
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

    /// Whether `ty` mentions `type_param` — either structurally (params,
    /// return type, and everything else the shared content walk already
    /// covers) or as a signature's type-predicate target (`x is T`).
    ///
    /// The shared `ChildPolicy::CONTENT_PREDICATE` walk backing
    /// `type_contains_type_parameter_binder` deliberately does not descend
    /// into a signature's type-predicate target (its own doc calls this
    /// exclusion a preserved historical accident, "not known to be
    /// semantic"), so a callback argument whose only channel for a type
    /// parameter is a predicate — e.g. `isApplicable: (v: any) => v is I` —
    /// is invisible to that walk. This evidence site needs to see it: such a
    /// predicate is `tsc`'s primary inference source for `I`, so treating it
    /// as "no evidence" defaults `I` to `unknown` and produces spurious
    /// TS18046/TS2698 diagnostics inside sibling callback bodies that
    /// legitimately depend on `I`.
    fn type_or_predicate_target_mentions_type_param(
        &mut self,
        ty: TypeId,
        type_param: tsz_solver::TypeParamInfo,
    ) -> bool {
        if crate::query_boundaries::generic_instantiation::type_contains_type_parameter_binder(
            self.ctx.types,
            ty,
            type_param,
        ) {
            return true;
        }
        self.resolved_callback_contextual_signature(ty)
            .and_then(|shape| shape.type_predicate)
            .and_then(|predicate| predicate.type_id)
            .is_some_and(|predicate_type| {
                crate::query_boundaries::generic_instantiation::type_contains_type_parameter_binder(
                    self.ctx.types,
                    predicate_type,
                    type_param,
                )
            })
    }
}
