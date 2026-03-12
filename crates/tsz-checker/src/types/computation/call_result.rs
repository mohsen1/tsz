//! Call-result handling helpers shared by call expression computation.

use crate::query_boundaries::assignability as assign_query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::visitor;
use tsz_solver::{CallResult, TypeData, TypeId};

pub(super) struct CallResultContext<'a> {
    pub(super) callee_expr: NodeIndex,
    pub(super) call_idx: NodeIndex,
    pub(super) args: &'a [NodeIndex],
    pub(super) arg_types: &'a [TypeId],
    pub(super) callee_type: TypeId,
    pub(super) is_super_call: bool,
    pub(super) is_optional_chain: bool,
    pub(super) allow_contextual_mismatch_deferral: bool,
}

impl<'a> CheckerState<'a> {
    fn finalize_call_return_like_success(
        &mut self,
        callee_expr: NodeIndex,
        arg_types: &[TypeId],
        return_type: TypeId,
        is_optional_chain: bool,
    ) -> TypeId {
        let return_type = self.apply_this_substitution_to_call_return(return_type, callee_expr);
        let return_type = self.refine_mixin_call_return_type(callee_expr, arg_types, return_type);
        let return_type = if !self.ctx.compiler_options.sound_mode {
            tsz_solver::relations::freshness::widen_freshness(self.ctx.types, return_type)
        } else {
            return_type
        };
        if is_optional_chain {
            self.ctx
                .types
                .factory()
                .union(vec![return_type, TypeId::UNDEFINED])
        } else {
            return_type
        }
    }

    fn stable_call_recovery_return_type(&self, callee_type: TypeId) -> Option<TypeId> {
        match self.ctx.types.lookup(callee_type) {
            Some(TypeData::Function(shape_id)) => {
                Some(self.ctx.types.function_shape(shape_id).return_type)
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                let first = shape.call_signatures.first()?.return_type;
                if shape
                    .call_signatures
                    .iter()
                    .all(|sig| sig.return_type == first)
                {
                    Some(first)
                } else {
                    None
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                let mut candidate = None;
                for &member in members.iter() {
                    let Some(return_type) = self.stable_call_recovery_return_type(member) else {
                        continue;
                    };
                    if let Some(existing) = candidate {
                        if existing != return_type {
                            return None;
                        }
                    } else {
                        candidate = Some(return_type);
                    }
                }
                candidate
            }
            _ => None,
        }
    }

    fn should_attempt_deferred_literal_elaboration(&mut self, expected: TypeId) -> bool {
        let expected = self.evaluate_type_with_env(expected);
        let expected = self.resolve_type_for_property_access(expected);
        let expected = self.resolve_lazy_type(expected);
        let expected = self.evaluate_application_type(expected);
        crate::query_boundaries::common::contains_never_type(self.ctx.types, expected)
    }

    fn argument_supports_literal_elaboration(&self, arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        )
    }

    fn preferred_literal_expected_for_mismatch(
        &self,
        arg_types: &[TypeId],
        mismatch_index: usize,
        expected: TypeId,
    ) -> TypeId {
        if tsz_solver::literal_value(self.ctx.types, expected).is_some() {
            return expected;
        }

        arg_types
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != mismatch_index)
            .map(|(_, ty)| *ty)
            .find(|&candidate| {
                tsz_solver::literal_value(self.ctx.types, candidate).is_some()
                    && tsz_solver::widen_literal_type(self.ctx.types, candidate) == expected
            })
            .unwrap_or(expected)
    }

    fn is_generic_callable_against_nongeneric_target(
        &self,
        actual: TypeId,
        expected: TypeId,
    ) -> bool {
        let Some(source_fn) = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            actual,
        ) else {
            return false;
        };
        let Some(target_fn) = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        ) else {
            return false;
        };
        !source_fn.type_params.is_empty() && target_fn.type_params.is_empty()
    }

    fn generic_callable_mismatch_display_target(
        &self,
        actual: TypeId,
        expected: TypeId,
    ) -> Option<TypeId> {
        let source_fn = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            actual,
        )?;
        let target_fn = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        )?;
        if source_fn.type_params.is_empty() || !target_fn.type_params.is_empty() {
            return None;
        }

        let tracked_type_params: FxHashSet<_> =
            source_fn.type_params.iter().map(|tp| tp.name).collect();
        let mut substitution = tsz_solver::TypeSubstitution::new();

        for (source_param, target_param) in source_fn.params.iter().zip(target_fn.params.iter()) {
            let target_type = if target_param.optional {
                self.ctx
                    .types
                    .factory()
                    .union(vec![target_param.type_id, TypeId::UNDEFINED])
            } else {
                target_param.type_id
            };
            if matches!(target_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
                continue;
            }

            for ty in visitor::collect_all_types(self.ctx.types, source_param.type_id) {
                let Some(tp) = tsz_solver::type_param_info(self.ctx.types, ty) else {
                    continue;
                };
                if tracked_type_params.contains(&tp.name) && substitution.get(tp.name).is_none() {
                    substitution.insert(tp.name, target_type);
                }
            }
        }

        if substitution.is_empty() {
            return None;
        }

        let return_type =
            tsz_solver::instantiate_type(self.ctx.types, source_fn.return_type, &substitution);
        Some(
            self.ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape {
                    type_params: vec![],
                    params: target_fn.params.clone(),
                    this_type: target_fn.this_type,
                    return_type,
                    type_predicate: target_fn.type_predicate.clone(),
                    is_constructor: target_fn.is_constructor,
                    is_method: target_fn.is_method,
                }),
        )
    }

    /// Handle the result of a call evaluation, emitting diagnostics for errors
    /// and applying this-substitution/mixin refinement for successes.
    pub(super) fn handle_call_result(
        &mut self,
        result: CallResult,
        context: CallResultContext<'_>,
    ) -> TypeId {
        let CallResultContext {
            callee_expr,
            call_idx,
            args,
            arg_types,
            callee_type,
            is_super_call,
            is_optional_chain,
            allow_contextual_mismatch_deferral,
            ..
        } = context;
        match result {
            CallResult::Success(return_type) => {
                if is_super_call {
                    return TypeId::VOID;
                }
                self.finalize_call_return_like_success(
                    callee_expr,
                    arg_types,
                    return_type,
                    is_optional_chain,
                )
            }
            CallResult::NonVoidFunctionCalledWithNew | CallResult::VoidFunctionCalledWithNew => {
                self.error_non_void_function_called_with_new_at(callee_expr);
                TypeId::ANY
            }
            CallResult::NotCallable { .. } => {
                if is_super_call {
                    return TypeId::VOID;
                }
                if self.is_constructor_type(callee_type) {
                    self.error_class_constructor_without_new_at(callee_type, callee_expr);
                } else if self.is_get_accessor_call(callee_expr) {
                    self.error_get_accessor_not_callable_at(callee_expr);
                } else if self.ctx.compiler_options.strict_null_checks {
                    let (_non_nullish, nullish_cause) = self.split_nullish_type(callee_type);
                    if let Some(cause) = nullish_cause {
                        self.error_cannot_invoke_possibly_nullish_at(cause, callee_expr);
                    } else {
                        self.error_not_callable_at(callee_type, callee_expr);
                    }
                } else {
                    self.error_not_callable_at(callee_type, callee_expr);
                }
                TypeId::ERROR
            }
            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                if !self.ctx.has_parse_errors {
                    if actual < expected_min {
                        let is_iife = self.is_callee_function_expression(callee_expr);
                        if is_iife {
                            return TypeId::ERROR;
                        }
                    }

                    let has_non_tuple_spread = args.iter().any(|&arg_idx| {
                        if let Some(n) = self.ctx.arena.get(arg_idx)
                            && n.kind == syntax_kind_ext::SPREAD_ELEMENT
                            && let Some(spread_data) = self.ctx.arena.get_spread(n)
                        {
                            let spread_type = self.get_type_of_node(spread_data.expression);
                            let spread_type = self.resolve_type_for_property_access(spread_type);
                            let spread_type = self.resolve_lazy_type(spread_type);
                            crate::query_boundaries::common::tuple_elements(
                                self.ctx.types,
                                spread_type,
                            )
                            .is_none()
                        } else {
                            false
                        }
                    });
                    if has_non_tuple_spread {
                    } else if actual < expected_min && expected_max.is_none() {
                        self.error_expected_at_least_arguments_at(expected_min, actual, call_idx);
                    } else {
                        let max = expected_max.unwrap_or(expected_min);
                        let expanded_args = self.build_expanded_args_for_error(args);
                        let args_for_error = if expanded_args.len() > args.len() {
                            &expanded_args
                        } else {
                            args
                        };
                        self.error_argument_count_mismatch_at(
                            expected_min,
                            max,
                            actual,
                            call_idx,
                            args_for_error,
                        );
                    }
                }
                if is_super_call {
                    TypeId::VOID
                } else if let Some(return_type) = self.stable_call_recovery_return_type(callee_type)
                {
                    self.finalize_call_return_like_success(
                        callee_expr,
                        arg_types,
                        return_type,
                        is_optional_chain,
                    )
                } else {
                    TypeId::ERROR
                }
            }
            CallResult::OverloadArgumentCountMismatch {
                actual,
                expected_low,
                expected_high,
            } => {
                if !self.ctx.has_parse_errors {
                    self.error_at_node(
                        call_idx,
                        &format!(
                            "No overload expects {actual} arguments, but overloads do exist that expect either {expected_low} or {expected_high} arguments."
                        ),
                        diagnostic_codes::NO_OVERLOAD_EXPECTS_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR_ARGUM,
                    );
                }
                TypeId::ERROR
            }
            CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
                fallback_return,
            } => {
                if actual == TypeId::ERROR
                    || actual == TypeId::UNKNOWN
                    || expected == TypeId::ERROR
                    || expected == TypeId::UNKNOWN
                {
                    return TypeId::ERROR;
                }

                let arg_idx = self.map_expanded_arg_index_to_original(args, index);
                let mismatch_is_spread_arg = arg_idx.is_some_and(|arg_idx| {
                    self.ctx
                        .arena
                        .get(arg_idx)
                        .is_some_and(|node| node.kind == syntax_kind_ext::SPREAD_ELEMENT)
                });
                if mismatch_is_spread_arg {
                    let normalized_rest_expected =
                        self.rest_argument_element_type_with_env(expected);
                    if normalized_rest_expected != expected
                        && self.is_assignable_to_with_env(actual, normalized_rest_expected)
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                }
                let reported_actual = arg_types.get(index).copied().unwrap_or(actual);
                let reported_expected = self
                    .generic_callable_mismatch_display_target(actual, expected)
                    .unwrap_or(expected);
                let reported_expected = self.preferred_literal_expected_for_mismatch(
                    arg_types,
                    index,
                    reported_expected,
                );
                let mut elaborated = false;
                let should_try_deferred_elaboration = self
                    .should_attempt_deferred_literal_elaboration(expected)
                    || arg_idx
                        .is_some_and(|arg_idx| self.argument_supports_literal_elaboration(arg_idx));
                if let Some(arg_idx) = arg_idx {
                    self.suppress_later_call_excess_property_diagnostics(args, arg_idx);
                    if should_try_deferred_elaboration
                        && !self.should_suppress_weak_key_arg_mismatch(
                            callee_expr,
                            args,
                            index,
                            actual,
                        )
                    {
                        elaborated = self.try_elaborate_object_literal_arg_error(arg_idx, expected);
                    }
                    if !elaborated
                        && allow_contextual_mismatch_deferral
                        && self.should_defer_contextual_argument_mismatch(actual, expected)
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    if !self.should_suppress_weak_key_arg_mismatch(callee_expr, args, index, actual)
                        && !elaborated
                    {
                        let _ = self.check_argument_assignable_or_report(
                            reported_actual,
                            reported_expected,
                            arg_idx,
                        );
                    }
                } else if !args.is_empty() {
                    let last_arg = args[args.len() - 1];
                    if should_try_deferred_elaboration
                        && !self.should_suppress_weak_key_arg_mismatch(
                            callee_expr,
                            args,
                            index,
                            actual,
                        )
                    {
                        elaborated =
                            self.try_elaborate_object_literal_arg_error(last_arg, expected);
                    }
                    if !elaborated
                        && allow_contextual_mismatch_deferral
                        && self.should_defer_contextual_argument_mismatch(actual, expected)
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    if !self.should_suppress_weak_key_arg_mismatch(callee_expr, args, index, actual)
                        && !elaborated
                    {
                        let _ = self.check_argument_assignable_or_report(
                            reported_actual,
                            reported_expected,
                            last_arg,
                        );
                    }
                } else {
                    if allow_contextual_mismatch_deferral
                        && self.should_defer_contextual_argument_mismatch(actual, expected)
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    let _ = self.check_argument_assignable_or_report(
                        reported_actual,
                        reported_expected,
                        call_idx,
                    );
                }

                if self.is_generic_callable_against_nongeneric_target(actual, expected) {
                    TypeId::UNKNOWN
                } else if fallback_return != TypeId::ERROR {
                    fallback_return
                } else if let Some(return_type) =
                    crate::query_boundaries::assignability::get_function_return_type(
                        self.ctx.types,
                        callee_type,
                    )
                {
                    self.apply_this_substitution_to_call_return(return_type, callee_expr)
                } else {
                    TypeId::ERROR
                }
            }
            CallResult::TypeParameterConstraintViolation {
                inferred_type,
                constraint_type,
                return_type,
            } => {
                let _ = self.check_assignable_or_report_generic_at(
                    inferred_type,
                    constraint_type,
                    call_idx,
                    call_idx,
                );
                return_type
            }
            CallResult::NoOverloadMatch {
                failures,
                fallback_return,
                ..
            } => {
                let has_error_surface = self.type_contains_error(callee_type)
                    || args.iter().copied().any(|arg_idx| {
                        let arg_type = self.get_type_of_node(arg_idx);
                        arg_type == TypeId::ERROR || self.type_contains_error(arg_type)
                    });
                if has_error_surface {
                    return TypeId::ERROR;
                }
                if !self.should_suppress_weak_key_no_overload(callee_expr, args) {
                    self.error_no_overload_matches_at(call_idx, &failures);
                }
                fallback_return
            }
            CallResult::ThisTypeMismatch {
                expected_this,
                actual_this,
            } => {
                self.error_this_type_mismatch_at(expected_this, actual_this, callee_expr);
                TypeId::ERROR
            }
        }
    }

    pub(crate) fn should_defer_contextual_argument_mismatch(
        &self,
        actual: TypeId,
        expected: TypeId,
    ) -> bool {
        let callable_mismatch = tsz_solver::type_queries::is_callable_type(self.ctx.types, actual)
            && tsz_solver::type_queries::is_callable_type(self.ctx.types, expected);
        if assign_query::contains_infer_types(self.ctx.types, actual)
            || assign_query::contains_infer_types(self.ctx.types, expected)
        {
            return true;
        }
        if !callable_mismatch
            && assign_query::contains_type_parameters(self.ctx.types, expected)
            && assign_query::contains_any_type(self.ctx.types, actual)
        {
            return true;
        }
        if !callable_mismatch
            && assign_query::contains_type_parameters(self.ctx.types, actual)
            && assign_query::contains_type_parameters(self.ctx.types, expected)
        {
            return true;
        }
        assign_query::is_any_type(self.ctx.types, expected)
    }

    pub(crate) fn suppress_later_call_excess_property_diagnostics(
        &mut self,
        args: &[NodeIndex],
        primary_arg_idx: NodeIndex,
    ) {
        let Some(primary_pos) = args.iter().position(|&arg| arg == primary_arg_idx) else {
            return;
        };
        let later_spans: Vec<(u32, u32)> = args[primary_pos + 1..]
            .iter()
            .filter_map(|&arg_idx| {
                self.get_node_span(arg_idx)
                    .map(|(start, len)| (start, start.saturating_add(len)))
            })
            .collect();
        if later_spans.is_empty() {
            return;
        }
        self.ctx.diagnostics.retain(|diag| {
            if diag.code
                != diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
            {
                return true;
            }
            !later_spans
                .iter()
                .any(|&(start, end)| diag.start >= start && diag.start < end)
        });
    }

    fn is_callee_function_expression(&self, callee_expr: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };
        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION => true,
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.is_callee_function_expression(paren.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub(crate) fn build_expanded_args_for_error(&mut self, args: &[NodeIndex]) -> Vec<NodeIndex> {
        let mut expanded = Vec::with_capacity(args.len());
        for &arg_idx in args {
            if let Some(n) = self.ctx.arena.get(arg_idx)
                && n.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread_data) = self.ctx.arena.get_spread(n)
            {
                let spread_type = self.get_type_of_node(spread_data.expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);
                let spread_type = self.resolve_lazy_type(spread_type);
                if let Some(elems) =
                    crate::query_boundaries::common::tuple_elements(self.ctx.types, spread_type)
                {
                    for _ in &elems {
                        expanded.push(arg_idx);
                    }
                    continue;
                }
            }
            expanded.push(arg_idx);
        }
        expanded
    }
}
