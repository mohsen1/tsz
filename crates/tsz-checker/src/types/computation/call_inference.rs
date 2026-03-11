//! Generic-call inference and round-2 contextual typing helpers.

use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::checkers::call::is_type_parameter_type;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tracing::trace;
use tsz_common::Atom;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{CallResult, FunctionShape, TypeId};

/// Count the number of non-`any` parameter types in a callable type.
///
/// Used to compare contextual type candidates: whichever has more specific
/// (non-`any`) parameter types provides better contextual typing for callbacks.
/// Returns 0 for non-callable types.
fn callable_param_specificity(db: &dyn tsz_solver::QueryDatabase, ty: TypeId) -> usize {
    if let Some(shape) = tsz_solver::type_queries::get_function_shape(db, ty) {
        shape
            .params
            .iter()
            .filter(|p| p.type_id != TypeId::ANY)
            .count()
    } else {
        0
    }
}

fn sanitize_function_shape_binding_pattern_params(
    shape: &tsz_solver::FunctionShape,
    binding_pattern_param_positions: &[usize],
) -> tsz_solver::FunctionShape {
    let mut params = shape.params.clone();
    for &index in binding_pattern_param_positions {
        if let Some(param) = params.get_mut(index) {
            param.type_id = TypeId::UNKNOWN;
        }
    }
    tsz_solver::FunctionShape {
        type_params: shape.type_params.clone(),
        params,
        this_type: shape.this_type,
        return_type: shape.return_type,
        type_predicate: shape.type_predicate.clone(),
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    }
}

pub(crate) fn should_preserve_contextual_application_shape(
    db: &dyn tsz_solver::TypeDatabase,
    ty: TypeId,
) -> bool {
    if tsz_solver::type_queries::get_application_info(db, ty).is_some() {
        return true;
    }

    if let Some(members) = tsz_solver::type_queries::get_union_members(db, ty) {
        return members
            .iter()
            .copied()
            .any(|member| should_preserve_contextual_application_shape(db, member));
    }

    if let Some(inner) =
        tsz_solver::readonly_inner_type(db, ty).or_else(|| tsz_solver::no_infer_inner_type(db, ty))
    {
        return should_preserve_contextual_application_shape(db, inner);
    }

    false
}

fn instantiate_function_shape_with_substitution(
    types: &dyn tsz_solver::QueryDatabase,
    func: &tsz_solver::FunctionShape,
    substitution: &tsz_solver::TypeSubstitution,
) -> tsz_solver::FunctionShape {
    tsz_solver::FunctionShape {
        params: func
            .params
            .iter()
            .map(|param| tsz_solver::ParamInfo {
                name: param.name,
                type_id: tsz_solver::instantiate_type(types, param.type_id, substitution),
                optional: param.optional,
                rest: param.rest,
            })
            .collect(),
        return_type: tsz_solver::instantiate_type(types, func.return_type, substitution),
        this_type: func
            .this_type
            .map(|this_type| tsz_solver::instantiate_type(types, this_type, substitution)),
        type_params: vec![],
        type_predicate: func
            .type_predicate
            .as_ref()
            .map(|predicate| tsz_solver::TypePredicate {
                asserts: predicate.asserts,
                target: predicate.target.clone(),
                type_id: predicate
                    .type_id
                    .map(|tid| tsz_solver::instantiate_type(types, tid, substitution)),
                parameter_index: predicate.parameter_index,
            }),
        is_constructor: func.is_constructor,
        is_method: func.is_method,
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn rest_argument_element_type_with_env(&mut self, type_id: TypeId) -> TypeId {
        let evaluated = self.evaluate_type_with_env(type_id);
        tsz_solver::rest_argument_element_type(self.ctx.types, evaluated)
    }

    pub(crate) fn target_contains_blocking_return_context_type_params(
        &self,
        target: TypeId,
        tracked_type_params: &FxHashSet<Atom>,
    ) -> bool {
        if tsz_solver::type_queries::contains_infer_types_db(self.ctx.types, target) {
            return true;
        }

        tsz_solver::collect_referenced_types(self.ctx.types, target)
            .into_iter()
            .filter_map(|ty| tsz_solver::type_param_info(self.ctx.types, ty))
            .any(|info| tracked_type_params.contains(&info.name))
    }

    fn instantiate_contextual_constraint_without_unresolved_self(
        &mut self,
        type_param_type: TypeId,
        tp_info: &tsz_solver::TypeParamInfo,
        substitution: &tsz_solver::TypeSubstitution,
    ) -> Option<TypeId> {
        let constraint = tp_info.constraint?;
        let should_drop_self = substitution.get(tp_info.name).is_some_and(|ty| {
            ty == TypeId::ERROR
                || ty == TypeId::UNKNOWN
                || tsz_solver::type_queries::contains_infer_types_db(self.ctx.types, ty)
        });
        let mut contextual_substitution = tsz_solver::TypeSubstitution::new();
        for (&name, &type_id) in substitution.map() {
            let aliases_current_type_param = type_id == type_param_type;
            if (should_drop_self && name == tp_info.name) || aliases_current_type_param {
                continue;
            }
            contextual_substitution.insert(name, type_id);
        }
        Some(tsz_solver::instantiate_type(
            self.ctx.types,
            constraint,
            &contextual_substitution,
        ))
    }

    fn unresolved_contextual_substitution_target(
        &self,
        tp_info: &tsz_solver::TypeParamInfo,
        substitution: &tsz_solver::TypeSubstitution,
    ) -> Option<TypeId> {
        let ty = substitution.get(tp_info.name)?;
        (ty == TypeId::ERROR
            || ty == TypeId::UNKNOWN
            || tsz_solver::type_queries::contains_infer_types_db(self.ctx.types, ty))
        .then_some(ty)
    }

    pub(crate) fn instantiate_generic_function_argument_against_target_for_refinement(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> TypeId {
        let source_eval = self.evaluate_type_with_env(source_ty);
        let target_eval = self.evaluate_type_with_env(target_ty);
        let function_info = match (
            call_checker::get_contextual_signature(self.ctx.types, source_ty),
            call_checker::get_contextual_signature(self.ctx.types, target_ty),
        ) {
            (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
            _ => match (
                call_checker::get_contextual_signature(self.ctx.types, source_eval),
                call_checker::get_contextual_signature(self.ctx.types, target_eval),
            ) {
                (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
                _ => None,
            },
        };

        let Some((source_fn, target_fn)) = function_info else {
            return source_ty;
        };
        if source_fn.type_params.is_empty() || source_fn.params.len() > target_fn.params.len() {
            return source_ty;
        }

        let target_param_types: Vec<_> = target_fn
            .params
            .iter()
            .take(source_fn.params.len())
            .map(|p| p.type_id)
            .collect();
        let env = self.ctx.type_env.borrow();
        let substitution = call_checker::compute_contextual_types_with_context(
            self.ctx.types,
            &self.ctx,
            &env,
            &source_fn,
            &target_param_types,
            Some(target_ty),
        );
        let instantiated =
            instantiate_function_shape_with_substitution(self.ctx.types, &source_fn, &substitution);
        self.ctx.types.factory().function(instantiated)
    }

    pub(crate) fn collect_return_context_substitution(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<Atom>,
        substitution: &mut tsz_solver::TypeSubstitution,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) {
        if !visited.insert((source, target)) {
            return;
        }

        if let Some(tp) = tsz_solver::type_param_info(self.ctx.types, source)
            && tracked_type_params.contains(&tp.name)
            && target != TypeId::UNKNOWN
            && target != TypeId::ERROR
            && !self
                .target_contains_blocking_return_context_type_params(target, tracked_type_params)
        {
            if substitution.get(tp.name).is_none() {
                substitution.insert(tp.name, target);
            }
            return;
        }

        if let Some(target_members) =
            tsz_solver::type_queries::get_union_members(self.ctx.types, target)
        {
            let before_len = substitution.len();
            for member in target_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
            {
                self.collect_return_context_substitution(
                    source,
                    member,
                    tracked_type_params,
                    substitution,
                    visited,
                );
                if substitution.len() > before_len {
                    return;
                }
            }
        }

        if let Some(inner) = tsz_solver::readonly_inner_type(self.ctx.types, target)
            .or_else(|| tsz_solver::no_infer_inner_type(self.ctx.types, target))
        {
            self.collect_return_context_substitution(
                source,
                inner,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        if let Some(inner) = tsz_solver::readonly_inner_type(self.ctx.types, source)
            .or_else(|| tsz_solver::no_infer_inner_type(self.ctx.types, source))
        {
            self.collect_return_context_substitution(
                inner,
                target,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        let source_eval = self.evaluate_type_with_env(source);
        let target_eval = self.evaluate_type_with_env(target);
        let function_info = match (
            call_checker::get_contextual_signature(self.ctx.types, source),
            call_checker::get_contextual_signature(self.ctx.types, target),
        ) {
            (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
            _ => match (
                call_checker::get_contextual_signature(self.ctx.types, source_eval),
                call_checker::get_contextual_signature(self.ctx.types, target_eval),
            ) {
                (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
                _ => None,
            },
        };

        if let Some((source_fn, target_fn)) = function_info
            && source_fn.params.len() <= target_fn.params.len()
        {
            for (source_param, target_param) in source_fn.params.iter().zip(target_fn.params.iter())
            {
                self.collect_return_context_substitution(
                    source_param.type_id,
                    target_param.type_id,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            self.collect_return_context_substitution(
                source_fn.return_type,
                target_fn.return_type,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let (Some(source_elems), Some(target_elems)) = (
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, source),
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, target),
        ) {
            for (source_elem, target_elem) in source_elems.iter().zip(target_elems.iter()) {
                self.collect_return_context_substitution(
                    source_elem.type_id,
                    target_elem.type_id,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            return;
        }

        if let (Some(source_elem), Some(target_elem)) = (
            tsz_solver::type_queries::get_array_element_type(self.ctx.types, source),
            tsz_solver::type_queries::get_array_element_type(self.ctx.types, target),
        ) {
            self.collect_return_context_substitution(
                source_elem,
                target_elem,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) =
            tsz_solver::type_queries::get_array_element_type(self.ctx.types, source)
            && let Some((_target_base, target_args)) =
                tsz_solver::type_queries::get_application_info(self.ctx.types, target)
            && target_args.len() == 1
        {
            self.collect_return_context_substitution(
                source_elem,
                target_args[0],
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) =
            tsz_solver::type_queries::get_array_element_type(self.ctx.types, source)
            && let Some(iterator_info) =
                tsz_solver::operations::get_iterator_info(self.ctx.types, target, false)
        {
            self.collect_return_context_substitution(
                source_elem,
                iterator_info.yield_type,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let (Some((source_base, source_args)), Some((target_base, target_args))) = (
            tsz_solver::type_queries::get_application_info(self.ctx.types, source),
            tsz_solver::type_queries::get_application_info(self.ctx.types, target),
        ) && source_base == target_base
            && source_args.len() == target_args.len()
        {
            for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                self.collect_return_context_substitution(
                    *source_arg,
                    *target_arg,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
        }
    }

    pub(crate) fn compute_return_context_substitution_from_shape(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        contextual_type: Option<TypeId>,
    ) -> tsz_solver::TypeSubstitution {
        let Some(contextual_type) = contextual_type else {
            return tsz_solver::TypeSubstitution::new();
        };
        let tracked_type_params: FxHashSet<_> =
            shape.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return tsz_solver::TypeSubstitution::new();
        }

        let mut substitution = tsz_solver::TypeSubstitution::new();
        let mut visited = FxHashSet::default();
        self.collect_return_context_substitution(
            shape.return_type,
            contextual_type,
            &tracked_type_params,
            &mut substitution,
            &mut visited,
        );
        substitution
    }

    pub(crate) fn zero_param_callback_first_conditional_branch(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::ARROW_FUNCTION
            && node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.ctx.arena.get_function(node)?;
        if !func.parameters.nodes.is_empty() || func.type_annotation.is_some() {
            return None;
        }

        let body_node = self.ctx.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::BLOCK {
            let block = self.ctx.arena.get_block(body_node)?;
            let ret_expr = block.statements.nodes.iter().find_map(|&stmt_idx| {
                let stmt_node = self.ctx.arena.get(stmt_idx)?;
                if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                    return None;
                }
                self.ctx
                    .arena
                    .get_return_statement(stmt_node)
                    .and_then(|ret| (ret.expression != NodeIndex::NONE).then_some(ret.expression))
            })?;
            let ret_node = self.ctx.arena.get(ret_expr)?;
            if ret_node.kind != syntax_kind_ext::CONDITIONAL_EXPRESSION {
                return None;
            }
            return self
                .ctx
                .arena
                .get_conditional_expr(ret_node)
                .map(|cond| cond.when_true);
        }

        if body_node.kind != syntax_kind_ext::CONDITIONAL_EXPRESSION {
            return None;
        }
        self.ctx
            .arena
            .get_conditional_expr(body_node)
            .map(|cond| cond.when_true)
    }

    pub(crate) fn sanitize_generic_inference_arg_type(
        &mut self,
        arg_idx: NodeIndex,
        arg_type: TypeId,
    ) -> TypeId {
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return arg_type;
        };
        if arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
        {
            return arg_type;
        }

        let Some(func) = self.ctx.arena.get_function(arg_node) else {
            return arg_type;
        };

        let mut binding_pattern_param_positions = Vec::new();
        for (index, &param_idx) in func.parameters.nodes.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if param.type_annotation.is_some() {
                continue;
            }
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                binding_pattern_param_positions.push(index);
            }
        }

        if binding_pattern_param_positions.is_empty() {
            return arg_type;
        }

        if let Some(shape) = tsz_solver::type_queries::get_function_shape(self.ctx.types, arg_type)
        {
            let sanitized = sanitize_function_shape_binding_pattern_params(
                &shape,
                &binding_pattern_param_positions,
            );
            self.ctx.types.factory().function(sanitized)
        } else if let Some(shape) =
            tsz_solver::type_queries::get_callable_shape(self.ctx.types, arg_type)
        {
            let mut sanitized = shape.as_ref().clone();
            sanitized.call_signatures = sanitized
                .call_signatures
                .iter()
                .map(|sig| tsz_solver::CallSignature {
                    type_params: sig.type_params.clone(),
                    params: sanitize_function_shape_binding_pattern_params(
                        &tsz_solver::FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate.clone(),
                            is_constructor: false,
                            is_method: sig.is_method,
                        },
                        &binding_pattern_param_positions,
                    )
                    .params,
                    this_type: sig.this_type,
                    return_type: sig.return_type,
                    type_predicate: sig.type_predicate.clone(),
                    is_method: sig.is_method,
                })
                .collect();
            self.ctx.types.factory().callable(sanitized)
        } else {
            arg_type
        }
    }

    pub(crate) fn inference_type_is_anyish(
        &self,
        ty: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if !visited.insert(ty) {
            return false;
        }

        if ty == TypeId::ANY {
            return true;
        }

        if let Some(elem) = tsz_solver::type_queries::get_array_element_type(self.ctx.types, ty) {
            return self.inference_type_is_anyish(elem, visited);
        }

        if let Some(elems) = tsz_solver::type_queries::get_tuple_elements(self.ctx.types, ty) {
            return elems
                .iter()
                .all(|elem| self.inference_type_is_anyish(elem.type_id, visited));
        }

        if let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, ty) {
            return !members.is_empty()
                && members
                    .iter()
                    .all(|member| self.inference_type_is_anyish(*member, visited));
        }

        if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, ty)
        {
            return !members.is_empty()
                && members
                    .iter()
                    .all(|member| self.inference_type_is_anyish(*member, visited));
        }

        false
    }

    pub(crate) fn sanitize_generic_inference_arg_types(
        &mut self,
        args: &[NodeIndex],
        arg_types: &[TypeId],
    ) -> (Vec<TypeId>, bool) {
        let mut changed = false;
        let sanitized = args
            .iter()
            .zip(arg_types.iter().copied())
            .map(|(&arg_idx, arg_type)| {
                let sanitized = self.sanitize_generic_inference_arg_type(arg_idx, arg_type);
                if sanitized != arg_type {
                    changed = true;
                }
                sanitized
            })
            .collect();
        (sanitized, changed)
    }

    pub(crate) fn recheck_generic_call_arguments_with_real_types(
        &mut self,
        result: CallResult,
        instantiated_params: &[tsz_solver::ParamInfo],
        args: &[NodeIndex],
        arg_types: &[TypeId],
    ) -> CallResult {
        let expected_signature = (!instantiated_params.is_empty()).then(|| {
            self.ctx.types.factory().function(FunctionShape::new(
                instantiated_params.to_vec(),
                TypeId::UNKNOWN,
            ))
        });
        if !matches!(result, CallResult::Success(_)) {
            return result;
        }

        for (index, &cached_actual) in arg_types.iter().enumerate() {
            let expected = expected_signature.and_then(|signature| {
                self.contextual_parameter_type_for_call_with_env_from_expected(
                    signature,
                    index,
                    arg_types.len(),
                )
            });

            let Some(expected) = expected else {
                break;
            };

            let actual = args
                .get(index)
                .copied()
                .map(|arg_idx| self.refreshed_generic_call_arg_type(arg_idx, cached_actual))
                .unwrap_or(cached_actual);

            let is_assignable = self.is_assignable_to_with_env(actual, expected);

            if !is_assignable {
                return CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    fallback_return: TypeId::ERROR,
                };
            }
        }

        result
    }

    pub(crate) fn compute_round2_contextual_types(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        round1_instantiated_params: Option<&[tsz_solver::ParamInfo]>,
        sensitive_args: &[bool],
        current_substitution: &tsz_solver::TypeSubstitution,
        arg_count: usize,
    ) -> Vec<Option<TypeId>> {
        let mut round2_contextual_types: Vec<Option<TypeId>> = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            let shape_round2_param =
                shape
                    .params
                    .get(i)
                    .map(|p| (p.type_id, p.rest))
                    .or_else(|| {
                        let last = shape.params.last()?;
                        last.rest.then_some((last.type_id, true))
                    });
            let round2_param = round1_instantiated_params
                .and_then(|params| {
                    params.get(i).map(|p| (p.type_id, p.rest)).or_else(|| {
                        let last = params.last()?;
                        last.rest.then_some((last.type_id, true))
                    })
                })
                .or(shape_round2_param);
            let is_sensitive = i < sensitive_args.len() && sensitive_args[i];
            let round2_param = if is_sensitive {
                shape_round2_param.or(round2_param)
            } else {
                round2_param
            };
            let ctx_type = if let Some((param_type, is_rest_param)) = round2_param {
                let fresh_instantiated_from_shape =
                    shape_round2_param.map(|(shape_param_type, _)| {
                        tsz_solver::instantiate_type(
                            self.ctx.types,
                            shape_param_type,
                            current_substitution,
                        )
                    });
                let round1_has_unknown = param_type == TypeId::UNKNOWN
                    || tsz_solver::collect_referenced_types(self.ctx.types, param_type)
                        .contains(&TypeId::UNKNOWN);
                let round1_has_error = param_type == TypeId::ERROR
                    || tsz_solver::collect_referenced_types(self.ctx.types, param_type)
                        .contains(&TypeId::ERROR);
                let prefer_fresh_instantiation = is_sensitive
                    || round1_has_error
                    || tsz_solver::type_queries::contains_infer_types_db(
                        self.ctx.types,
                        param_type,
                    )
                    || tsz_solver::type_queries::contains_type_parameters_db(
                        self.ctx.types,
                        param_type,
                    )
                    || fresh_instantiated_from_shape.is_some_and(|fresh| {
                        (round1_has_unknown || round1_has_error)
                            && (tsz_solver::type_queries::contains_infer_types_db(
                                self.ctx.types,
                                fresh,
                            ) || tsz_solver::type_queries::contains_type_parameters_db(
                                self.ctx.types,
                                fresh,
                            ))
                    });
                let instantiated = if round1_instantiated_params.is_some()
                    && !prefer_fresh_instantiation
                {
                    let original_param = shape_round2_param.map(|(type_id, _)| type_id);
                    if let Some(orig) = original_param
                        && let Some(tp_info) = tsz_solver::type_param_info(self.ctx.types, orig)
                        && self
                            .unresolved_contextual_substitution_target(
                                &tp_info,
                                current_substitution,
                            )
                            .is_some()
                    {
                        let instantiated_constraint = match self
                            .instantiate_contextual_constraint_without_unresolved_self(
                                orig,
                                &tp_info,
                                current_substitution,
                            ) {
                            Some(instantiated_constraint) => instantiated_constraint,
                            None => param_type,
                        };
                        let evaluated_constraint =
                            self.evaluate_type_with_env(instantiated_constraint);
                        if !tsz_solver::type_queries::contains_type_parameters_db(
                            self.ctx.types,
                            evaluated_constraint,
                        ) {
                            let constraint_specificity =
                                callable_param_specificity(self.ctx.types, evaluated_constraint);
                            let round1_specificity =
                                callable_param_specificity(self.ctx.types, param_type);
                            if constraint_specificity >= round1_specificity {
                                evaluated_constraint
                            } else {
                                param_type
                            }
                        } else {
                            param_type
                        }
                    } else {
                        param_type
                    }
                } else {
                    let base_param_type = if prefer_fresh_instantiation {
                        shape_round2_param
                            .map(|(type_id, _)| type_id)
                            .unwrap_or(param_type)
                    } else {
                        param_type
                    };
                    let inst = if let Some(tp_info) =
                        tsz_solver::type_param_info(self.ctx.types, base_param_type)
                    {
                        if self
                            .unresolved_contextual_substitution_target(
                                &tp_info,
                                current_substitution,
                            )
                            .is_some()
                        {
                            self.instantiate_contextual_constraint_without_unresolved_self(
                                base_param_type,
                                &tp_info,
                                current_substitution,
                            )
                            .unwrap_or_else(|| {
                                tsz_solver::instantiate_type(
                                    self.ctx.types,
                                    base_param_type,
                                    current_substitution,
                                )
                            })
                        } else {
                            tsz_solver::instantiate_type(
                                self.ctx.types,
                                base_param_type,
                                current_substitution,
                            )
                        }
                    } else {
                        tsz_solver::instantiate_type(
                            self.ctx.types,
                            base_param_type,
                            current_substitution,
                        )
                    };
                    if let Some(tp_info) = tsz_solver::type_param_info(self.ctx.types, inst) {
                        let instantiated_constraint = match self
                            .instantiate_contextual_constraint_without_unresolved_self(
                                inst,
                                &tp_info,
                                current_substitution,
                            ) {
                            Some(instantiated_constraint) => instantiated_constraint,
                            None => inst,
                        };
                        let evaluated = self.evaluate_type_with_env(instantiated_constraint);
                        if !tsz_solver::type_queries::contains_type_parameters_db(
                            self.ctx.types,
                            evaluated,
                        ) {
                            evaluated
                        } else {
                            inst
                        }
                    } else {
                        inst
                    }
                };
                let preserve_application_shape =
                    should_preserve_contextual_application_shape(self.ctx.types, instantiated);
                let evaluated = if tsz_solver::type_queries::contains_type_parameters_db(
                    self.ctx.types,
                    instantiated,
                ) || tsz_solver::type_queries::contains_infer_types_db(
                    self.ctx.types,
                    instantiated,
                ) || preserve_application_shape
                {
                    instantiated
                } else {
                    self.evaluate_type_with_env(instantiated)
                };
                trace!(
                    arg_index = i,
                    preserve_application_shape,
                    param_type_id = param_type.0,
                    param_type_app_args = ?tsz_solver::type_queries::get_application_info(
                        self.ctx.types,
                        param_type,
                    )
                    .map(|(_, args)| args),
                    instantiated_id = instantiated.0,
                    instantiated_app_args = ?tsz_solver::type_queries::get_application_info(
                        self.ctx.types,
                        instantiated,
                    )
                    .map(|(_, args)| args),
                    evaluated_id = evaluated.0,
                    "Round 2: instantiated parameter type"
                );
                Some(if is_rest_param {
                    self.rest_argument_element_type_with_env(evaluated)
                } else {
                    evaluated
                })
            } else {
                None
            };
            trace!(
                arg_index = i,
                ctx_type_id = ?ctx_type.map(|t| t.0),
                "Round 2: contextual type for argument"
            );
            round2_contextual_types.push(ctx_type);
        }
        round2_contextual_types
    }

    pub(crate) fn compute_single_call_argument_type(
        &mut self,
        arg_idx: NodeIndex,
        expected_type: Option<TypeId>,
        check_excess_properties: bool,
        effective_index: usize,
        suppress_diagnostics: bool,
    ) -> TypeId {
        use tsz_scanner::SyntaxKind;

        let mut is_nested_invocation = false;
        let apply_contextual = {
            let Some(node) = self.ctx.arena.get(arg_idx) else {
                return TypeId::ERROR;
            };
            let is_literal = matches!(
                node.kind,
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::BigIntLiteral as u16
                    || k == SyntaxKind::TrueKeyword as u16
                    || k == SyntaxKind::FalseKeyword as u16
                    || k == SyntaxKind::NullKeyword as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            );
            if is_literal {
                true
            } else {
                is_nested_invocation = node.kind == syntax_kind_ext::CALL_EXPRESSION
                    || node.kind == syntax_kind_ext::NEW_EXPRESSION;
                matches!(
                    node.kind,
                    k if k == syntax_kind_ext::ARROW_FUNCTION
                        || k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                        || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                        || k == syntax_kind_ext::CALL_EXPRESSION
                        || k == syntax_kind_ext::NEW_EXPRESSION
                        || k == syntax_kind_ext::YIELD_EXPRESSION
                        || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                )
            }
        };
        let expected_context_type = if is_nested_invocation {
            expected_type
        } else {
            self.contextual_type_option_for_expression(expected_type)
        };

        let prev_context = self.ctx.contextual_type;
        if apply_contextual {
            self.ctx.contextual_type = expected_context_type;
        } else {
            self.ctx.contextual_type = None;
        }

        let skip_flow = if apply_contextual {
            false
        } else if let Some(node) = self.ctx.arena.get(arg_idx) {
            if node.kind != SyntaxKind::Identifier as u16 {
                false
            } else if let Some(sym_id) = self
                .ctx
                .binder
                .get_node_symbol(arg_idx)
                .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, arg_idx))
            {
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    let value_decl = symbol.value_declaration;
                    if value_decl.is_none() || !self.is_const_variable_declaration(value_decl) {
                        false
                    } else if let Some(decl_node) = self.ctx.arena.get(value_decl) {
                        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) {
                            if var_decl.type_annotation.is_some() || var_decl.initializer.is_none()
                            {
                                false
                            } else if let Some(init_node) = self.ctx.arena.get(var_decl.initializer)
                            {
                                init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                    || init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        let prev_skip_flow = self.ctx.skip_flow_narrowing;
        if skip_flow {
            self.ctx.skip_flow_narrowing = true;
        }

        let diag_len = self.ctx.diagnostics.len();
        let dedup_snapshot = suppress_diagnostics.then(|| self.ctx.emitted_diagnostics.clone());
        let arg_type = self.get_type_of_node(arg_idx);

        if skip_flow {
            self.ctx.skip_flow_narrowing = prev_skip_flow;
        }

        if check_excess_properties
            && let Some(expected) = expected_type
            && expected != TypeId::ANY
            && expected != TypeId::UNKNOWN
            && let Some(arg_node) = self.ctx.arena.get(arg_idx)
            && arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && !is_type_parameter_type(self.ctx.types, expected)
            && !self
                .ctx
                .generic_excess_skip
                .as_ref()
                .is_some_and(|skip| effective_index < skip.len() && skip[effective_index])
        {
            self.check_object_literal_excess_properties(arg_type, expected, arg_idx);
        }

        if suppress_diagnostics {
            let callback_body_start = self
                .ctx
                .arena
                .get(arg_idx)
                .and_then(|node| self.ctx.arena.get_function(node))
                .and_then(|func| self.ctx.arena.get(func.body))
                .filter(|body_node| body_node.kind != syntax_kind_ext::BLOCK)
                .map(|body_node| body_node.pos);
            let new_diags = self.ctx.diagnostics.split_off(diag_len);
            let kept_new_diags: Vec<_> = new_diags
                .into_iter()
                .filter(|diag| {
                    let is_provisional_implicit_any = matches!(
                        diag.code,
                        diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                            | diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                            | diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                            | diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                    );
                    let is_assignability = diag.code
                        == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                        || diag.code
                            == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
                    (!is_assignability && !is_provisional_implicit_any)
                        || callback_body_start.is_some_and(|start| diag.start == start)
                })
                .collect();
            self.ctx.diagnostics.extend(kept_new_diags);
            if let Some(dedup_snapshot) = dedup_snapshot {
                self.ctx.emitted_diagnostics = dedup_snapshot;
                for diag in self.ctx.diagnostics.iter().skip(diag_len) {
                    self.ctx.emitted_diagnostics.insert((diag.code, diag.start));
                }
            }
        }

        self.ctx.contextual_type = prev_context;
        arg_type
    }
}
