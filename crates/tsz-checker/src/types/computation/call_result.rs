//! Call-result handling helpers shared by call expression computation.

use crate::checkers_domain::call_checker::CallRelationEvidence;
use crate::query_boundaries::assignability as assign_query;
use crate::query_boundaries::common;
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{ParamInfo, TupleElement, TypeId};

pub(super) struct CallResultContext<'a> {
    pub(super) callee_expr: NodeIndex,
    pub(super) call_idx: NodeIndex,
    pub(super) args: &'a [NodeIndex],
    pub(super) arg_types: &'a [TypeId],
    pub(super) callee_type: TypeId,
    pub(super) callee_has_declared_generic_signature: bool,
    pub(super) is_super_call: bool,
    pub(super) is_optional_chain: bool,
    pub(super) allow_contextual_mismatch_deferral: bool,
    pub(super) relation_evidence: &'a [CallRelationEvidence],
}

impl<'a> CheckerState<'a> {
    fn relation_evidence_for_pair(
        relation_evidence: &[CallRelationEvidence],
        source: TypeId,
        target: TypeId,
    ) -> Option<&crate::query_boundaries::assignability::RelationOutcome> {
        relation_evidence
            .iter()
            .rev()
            .find(|evidence| evidence.source == source && evidence.target == target)
            .map(|evidence| &evidence.outcome)
    }

    pub(crate) fn report_argument_assignability_with_evidence(
        &mut self,
        relation_evidence: &[CallRelationEvidence],
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        if let Some(outcome) = Self::relation_evidence_for_pair(relation_evidence, source, target) {
            self.report_argument_assignability_with_outcome(
                source,
                target,
                arg_idx,
                outcome.clone(),
            )
        } else {
            self.check_argument_assignable_or_report(source, target, arg_idx)
        }
    }

    fn correlated_union_call_recovery_return(
        &mut self,
        callee_type: TypeId,
        arg_index: usize,
        actual: TypeId,
    ) -> Option<TypeId> {
        if !self.is_generic_indexed_access_surface(actual) {
            return None;
        }

        let signatures =
            common::get_call_signatures(self.ctx.types, callee_type).or_else(|| {
                self.ctx
                    .types
                    .get_display_alias(callee_type)
                    .and_then(|alias| common::get_call_signatures(self.ctx.types, alias))
            })?;
        if signatures.len() < 2 {
            return None;
        }

        let param_types: Vec<TypeId> = signatures
            .iter()
            .filter_map(|signature| signature.params.get(arg_index).map(|param| param.type_id))
            .collect();
        if param_types.len() < 2 {
            return None;
        }

        let param_union = self.ctx.types.factory().union(param_types);
        if !self.is_assignable_to(actual, param_union) {
            return None;
        }

        let return_types = signatures
            .iter()
            .map(|signature| signature.return_type)
            .collect();
        Some(self.ctx.types.factory().union(return_types))
    }

    fn normalized_builtin_object_entries_return_type(
        &self,
        callee_expr: NodeIndex,
        arg_types: &[TypeId],
        return_type: TypeId,
    ) -> TypeId {
        if arg_types.len() != 1 || arg_types[0] != TypeId::ANY {
            return return_type;
        }
        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return return_type;
        };
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return return_type;
        };
        if self.ctx.arena.get_identifier_text(access.name_or_argument) != Some("entries")
            || self.ctx.arena.get_identifier_text(access.expression) != Some("Object")
        {
            return return_type;
        }

        let tuple = self.ctx.types.factory().tuple(vec![
            tsz_solver::TupleElement {
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
                name: None,
            },
            tsz_solver::TupleElement {
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
                name: None,
            },
        ]);
        self.ctx.types.factory().array(tuple)
    }

    fn finalize_call_return_like_success(
        &mut self,
        callee_expr: NodeIndex,
        callee_type: TypeId,
        arg_types: &[TypeId],
        return_type: TypeId,
        is_optional_chain: bool,
    ) -> TypeId {
        let return_type = self.apply_this_substitution_to_call_return(return_type, callee_expr);
        let return_type =
            self.apply_direct_callable_this_substitution(return_type, callee_expr, callee_type);
        let return_type =
            self.refine_mixin_call_return_type(callee_expr, callee_type, arg_types, return_type);
        let return_type = if !self.ctx.compiler_options.sound_mode {
            common::widen_freshness(self.ctx.types, return_type)
        } else {
            return_type
        };
        // Eagerly evaluate monomorphic TypeApplications to avoid nested return
        // chains, but keep Promise-like applications wrapped for await handling.
        let return_type = if common::is_generic_application(self.ctx.types, return_type)
            && !self.contains_type_parameters_cached(return_type)
            && !self.is_promise_type(return_type)
        {
            self.evaluate_type_with_env(return_type)
        } else {
            return_type
        };
        if is_optional_chain {
            self.ctx
                .types
                .factory()
                .union2(return_type, TypeId::UNDEFINED)
        } else {
            return_type
        }
    }

    fn apply_direct_callable_this_substitution(
        &mut self,
        ty: TypeId,
        expr: NodeIndex,
        callee: TypeId,
    ) -> TypeId {
        if ty.is_intrinsic()
            || matches!(callee, TypeId::ERROR | TypeId::ANY)
            || !common::contains_this_type(self.ctx.types, ty)
            || self
                .ctx
                .arena
                .get(expr)
                .is_none_or(|node| node.kind != SyntaxKind::Identifier as u16)
        {
            ty
        } else {
            common::substitute_this_type_at_return_position(self.ctx.types, ty, callee)
        }
    }

    fn polymorphic_this_indexed_conditional_target(
        &mut self,
        callee_type: TypeId,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        index: usize,
    ) -> Option<TypeId> {
        if args.len() < 3 || arg_types.len() < 3 {
            return None;
        }
        if index != 2 {
            return None;
        }
        if self
            .ctx
            .arena
            .get(args[0])
            .is_none_or(|node| node.kind != tsz_scanner::SyntaxKind::ThisKeyword as u16)
        {
            return None;
        }

        let shape = common::function_shape_for_type(self.ctx.types, callee_type)
            .or_else(|| {
                common::function_shape_for_type(
                    self.ctx.types,
                    self.evaluate_type_with_env(callee_type),
                )
            })
            .or_else(|| {
                common::callable_shape_for_type(self.ctx.types, callee_type)
                    .and_then(|callable| callable.call_signatures.first().cloned())
                    .map(|sig| {
                        std::sync::Arc::new(tsz_solver::FunctionShape {
                            type_params: sig.type_params,
                            params: sig.params,
                            this_type: sig.this_type,
                            return_type: TypeId::UNKNOWN,
                            type_predicate: sig.type_predicate,
                            is_constructor: false,
                            is_method: sig.is_method,
                        })
                    })
            });
        let shape = shape?;
        if shape.type_params.len() < 2 || shape.params.len() < 3 {
            return None;
        }
        let first_param = common::type_param_info(self.ctx.types, shape.params[0].type_id)?;
        if first_param.name != shape.type_params[0].name {
            return None;
        }

        let third_param = shape.params[2].type_id;
        if !common::contains_type_parameters(self.ctx.types, third_param) {
            return None;
        }

        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        substitution.insert(shape.type_params[0].name, self.ctx.types.this_type());
        substitution.insert(shape.type_params[1].name, arg_types[1]);
        let target = crate::query_boundaries::common::instantiate_type_preserving_meta(
            self.ctx.types,
            third_param,
            &substitution,
        );
        if !common::contains_this_type(self.ctx.types, target) {
            return None;
        }
        Some(target)
    }

    fn report_polymorphic_this_indexed_conditional_arg(
        &mut self,
        callee_type: TypeId,
        args: &[NodeIndex],
        arg_types: &[TypeId],
    ) -> bool {
        let Some(target) =
            self.polymorphic_this_indexed_conditional_target(callee_type, args, arg_types, 2)
        else {
            return false;
        };
        if self.is_assignable_to(arg_types[2], target) {
            return false;
        }
        self.error_argument_not_assignable_preserving_param_display(arg_types[2], target, args[2]);
        true
    }

    fn error_argument_not_assignable_preserving_param_display(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) {
        if self.should_suppress_argument_not_assignable_diagnostic(arg_type, param_type) {
            return;
        }
        if self.should_suppress_self_referential_mapped_constraint_arg_mismatch(
            arg_type, param_type, arg_idx,
        ) {
            return;
        }

        let display_arg_type = common::widen_argument_type_for_display(self.ctx.types, arg_type);
        let mut actual_display = self.format_type_diagnostic(display_arg_type);
        if matches!(actual_display.as_str(), "true[]" | "false[]") {
            actual_display = "boolean[]".to_string();
        }
        let mut target_display = self
            .constrained_variadic_tuple_parameter_display(param_type, arg_type)
            .or_else(|| {
                self.underfilled_generic_variadic_tuple_parameter_display(param_type, arg_type)
            })
            .or_else(|| {
                self.finite_mapped_parameter_display_type(param_type)
                    .map(|display_type| self.format_type_for_assignability_message(display_type))
            })
            .unwrap_or_else(|| self.format_type_diagnostic(param_type));
        if target_display.contains("Array<") {
            target_display = Self::normalize_array_generic_to_shorthand(&target_display);
        }
        if let Some((generic_actual_display, generic_target_display)) =
            self.generic_direct_primitive_mismatch_display(arg_type, param_type, arg_idx)
        {
            actual_display = generic_actual_display;
            target_display = generic_target_display;
        }
        let message = format_message(
            diagnostic_messages::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
            &[&actual_display, &target_display],
        );
        self.error_at_node(
            arg_idx,
            &message,
            diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
        );
    }

    fn finite_mapped_parameter_display_type(&mut self, param_type: TypeId) -> Option<TypeId> {
        let mapped_id = common::mapped_type_id(self.ctx.types, param_type)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let names = crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
            self.ctx.types,
            mapped_id,
        )?;
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort_by(|a, b| {
            self.ctx
                .types
                .resolve_atom_ref(*a)
                .cmp(&self.ctx.types.resolve_atom_ref(*b))
        });

        let mut properties = Vec::with_capacity(names.len());
        for name in names {
            let property_name = self.ctx.types.resolve_atom_ref(name).to_string();
            let type_id =
                crate::query_boundaries::state::checking::get_finite_mapped_property_display_type(
                    self.ctx.types,
                    mapped_id,
                    &property_name,
                )?;
            let mut property = tsz_solver::PropertyInfo::new(name, type_id);
            property.optional = mapped.optional_modifier == Some(tsz_solver::MappedModifier::Add);
            property.readonly = mapped.readonly_modifier == Some(tsz_solver::MappedModifier::Add);
            properties.push(property);
        }

        Some(self.ctx.types.factory().object(properties))
    }

    fn stable_call_recovery_return_type(&self, callee_type: TypeId) -> Option<TypeId> {
        crate::query_boundaries::checkers::call::stable_call_recovery_return_type(
            self.ctx.types,
            callee_type,
        )
    }

    fn is_spread_argument_marker_type(&self, type_id: TypeId) -> bool {
        common::is_spread_marker_tuple(self.ctx.types.as_type_database(), type_id)
    }

    fn literalized_aggregate_actual_for_call_args(
        &mut self,
        args: &[NodeIndex],
        index: usize,
        actual: TypeId,
        expected: TypeId,
    ) -> Option<TypeId> {
        let actual_elements = common::tuple_elements(self.ctx.types, actual)?;
        let expanded_args = self.build_expanded_args_for_error(args);
        if index > expanded_args.len() {
            return None;
        }
        let rest_args = &expanded_args[index..];
        if rest_args.len() != actual_elements.len() {
            return None;
        }

        let mut changed = false;
        let elements: Vec<_> = actual_elements
            .into_iter()
            .enumerate()
            .zip(rest_args.iter().copied())
            .map(|((actual_pos, element), arg_idx)| {
                let expected_element = self.expected_tuple_element_for_aggregate_actual(
                    expected,
                    rest_args.len(),
                    actual_pos,
                );
                let should_widen =
                    expected_element.is_some_and(|ty| common::is_callable_type(self.ctx.types, ty));
                let type_id = if should_widen {
                    element.type_id
                } else {
                    self.literal_type_from_initializer(arg_idx)
                        .inspect(|&literal_type| {
                            changed |= literal_type != element.type_id;
                        })
                        .unwrap_or(element.type_id)
                };
                TupleElement {
                    type_id,
                    name: element.name,
                    optional: element.optional,
                    rest: element.rest,
                }
            })
            .collect();

        changed.then(|| self.ctx.types.tuple(elements))
    }

    pub(crate) fn inline_literal_satisfies_has_permissive_target(
        &mut self,
        arg_idx: NodeIndex,
    ) -> bool {
        let idx = self.ctx.arena.skip_parenthesized(arg_idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let satisfies_idx = if node.kind == syntax_kind_ext::SATISFIES_EXPRESSION {
            idx
        } else if matches!(
            node.kind,
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        ) {
            let Some(parent_idx) = self.ctx.arena.get_extended(idx).map(|info| info.parent) else {
                return false;
            };
            let parent_idx = self.ctx.arena.skip_parenthesized(parent_idx);
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent.kind != syntax_kind_ext::SATISFIES_EXPRESSION {
                return false;
            }
            parent_idx
        } else {
            return false;
        };
        let Some(assertion) = self
            .ctx
            .arena
            .get(satisfies_idx)
            .and_then(|node| self.ctx.arena.get_type_assertion(node))
        else {
            return false;
        };
        let Some(inner_node) = self
            .ctx
            .arena
            .get(self.ctx.arena.skip_parenthesized(assertion.expression))
        else {
            return false;
        };
        if !matches!(
            inner_node.kind,
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        ) {
            return false;
        }
        self.satisfies_target_is_permissive(assertion.type_node)
    }

    fn satisfies_target_is_permissive(&mut self, type_node: NodeIndex) -> bool {
        let type_node = self.ctx.arena.skip_parenthesized(type_node);
        if let Some(node) = self.ctx.arena.get(type_node)
            && matches!(
                node.kind,
                k if k == SyntaxKind::UnknownKeyword as u16
                    || k == SyntaxKind::AnyKeyword as u16
                    || k == SyntaxKind::NeverKeyword as u16
            )
        {
            return true;
        }
        matches!(
            self.get_type_from_type_node(type_node),
            TypeId::UNKNOWN | TypeId::ANY | TypeId::NEVER
        )
    }

    fn expected_tuple_element_for_aggregate_actual(
        &mut self,
        expected: TypeId,
        actual_len: usize,
        actual_pos: usize,
    ) -> Option<TypeId> {
        let expected = self.evaluate_type_with_env(expected);
        let expected = self.resolve_type_for_property_access(expected);
        let expected = self.resolve_lazy_type(expected);
        let expected = self.evaluate_application_type(expected);
        let expected = common::unwrap_readonly(self.ctx.types, expected);
        let elements = common::tuple_elements(self.ctx.types, expected)?;
        let rest_index = elements.iter().position(|element| element.rest)?;
        if actual_pos < rest_index {
            return elements.get(actual_pos).map(|element| element.type_id);
        }
        let tail = &elements[rest_index + 1..];
        if tail.is_empty() {
            return None;
        }
        let tail_start = actual_len.saturating_sub(tail.len());
        (actual_pos >= tail_start)
            .then(|| {
                tail.get(actual_pos - tail_start)
                    .map(|element| element.type_id)
            })
            .flatten()
    }

    fn declared_rest_parameter_index_for_call(&self, callee_expr: NodeIndex) -> Option<usize> {
        let callee_sym = self
            .resolve_identifier_symbol(callee_expr)
            .or_else(|| self.resolve_qualified_symbol(callee_expr))?;
        let callee = self.ctx.binder.get_symbol(callee_sym)?;
        callee.declarations.iter().copied().find_map(|decl_idx| {
            let node = self.ctx.arena.get(decl_idx)?;
            let func = self.ctx.arena.get_function(node)?;
            func.parameters
                .nodes
                .iter()
                .copied()
                .enumerate()
                .find_map(|(index, param_idx)| {
                    let param_node = self.ctx.arena.get(param_idx)?;
                    let param = self.ctx.arena.get_parameter(param_node)?;
                    param.dot_dot_dot_token.then_some(index)
                })
        })
    }

    fn aggregate_actual_after_declared_rest_start(
        &mut self,
        actual: TypeId,
        index: usize,
        declared_rest_index: usize,
    ) -> Option<TypeId> {
        if declared_rest_index <= index {
            return None;
        }
        let drop_count = declared_rest_index - index;
        let elements = common::tuple_elements(self.ctx.types, actual)?;
        if drop_count > elements.len() {
            return None;
        }
        Some(self.ctx.types.tuple(elements[drop_count..].to_vec()))
    }

    fn spread_rest_tuple_diagnostic_types(
        &mut self,
        arg_idx: NodeIndex,
        expected: TypeId,
    ) -> Option<(TypeId, TypeId)> {
        let arg_node = self.ctx.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
            return None;
        }
        let spread = self.ctx.arena.get_spread(arg_node)?;
        let mut spread_type = self.get_type_of_node(spread.expression);
        spread_type = self.resolve_type_for_property_access(spread_type);
        spread_type = self.resolve_lazy_type(spread_type);
        spread_type = self.evaluate_application_type(spread_type);
        common::array_element_type(self.ctx.types, spread_type)?;

        let mut callback_shape =
            (*common::function_shape_for_type(self.ctx.types, expected)?).clone();
        let last_param = callback_shape.params.last_mut()?;
        if !last_param.rest {
            return None;
        }
        *last_param = ParamInfo {
            type_id: spread_type,
            ..*last_param
        };
        let callback_type = self.ctx.types.factory().function(callback_shape);
        let expected_tuple = self.ctx.types.tuple(vec![
            TupleElement {
                type_id: spread_type,
                name: None,
                optional: false,
                rest: true,
            },
            TupleElement {
                type_id: callback_type,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        Some((spread_type, expected_tuple))
    }

    fn should_attempt_deferred_literal_elaboration(&mut self, expected: TypeId) -> bool {
        let expected = self.evaluate_type_with_env(expected);
        let expected = self.resolve_type_for_property_access(expected);
        let expected = self.resolve_lazy_type(expected);
        let expected = self.evaluate_application_type(expected);
        crate::query_boundaries::common::contains_never_type(self.ctx.types, expected)
    }

    pub(crate) fn argument_supports_literal_elaboration(&self, arg_idx: NodeIndex) -> bool {
        self.is_callback_like_argument(arg_idx)
            || self.ctx.arena.get(arg_idx).is_some_and(|node| {
                matches!(
                    node.kind,
                    k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                )
            })
    }

    fn callback_prefers_argument_level_return_mismatch(&self, arg_idx: NodeIndex) -> bool {
        let Some(func) = self
            .callback_function_index(arg_idx)
            .and_then(|idx| self.ctx.arena.get(idx))
            .and_then(|arg_node| self.ctx.arena.get_function(arg_node))
        else {
            return false;
        };
        self.ctx
            .arena
            .get(func.body)
            .is_some_and(|body| body.kind == syntax_kind_ext::BLOCK)
    }

    fn preferred_literal_expected_for_mismatch(
        &self,
        callee_has_declared_generic_signature: bool,
        arg_types: &[TypeId],
        args: &[NodeIndex],
        actual: TypeId,
        mismatch_index: usize,
        expected: TypeId,
    ) -> TypeId {
        if common::contains_type_parameters(self.ctx.types, expected) {
            return expected;
        }
        if common::literal_value(self.ctx.types, expected).is_some() {
            return expected;
        }
        let parameter_was_generic_target = self
            .ctx
            .generic_excess_skip
            .as_ref()
            .is_some_and(|skip| mismatch_index < skip.len() && skip[mismatch_index]);
        if let Some(expected) = self.generic_application_literal_expected_for_mismatch(
            callee_has_declared_generic_signature || parameter_was_generic_target,
            arg_types,
            args,
            expected,
        ) {
            return expected;
        }
        let actual_display_type = common::widen_argument_type_for_display(self.ctx.types, actual);
        if !common::is_primitive_type(self.ctx.types, actual_display_type) {
            return expected;
        }
        if !callee_has_declared_generic_signature {
            return expected;
        }
        arg_types
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != mismatch_index)
            .map(|(_, ty)| *ty)
            .find(|&candidate| {
                common::literal_value(self.ctx.types, candidate).is_some()
                    && common::widen_literal_type(self.ctx.types, candidate) == expected
            })
            .or_else(|| {
                args.iter()
                    .copied()
                    .enumerate()
                    .filter(|(idx, _)| *idx != mismatch_index)
                    .filter_map(|(_, arg_idx)| self.literal_type_from_initializer(arg_idx))
                    .find(|&candidate| {
                        common::widen_literal_type(self.ctx.types, candidate) == expected
                    })
            })
            .unwrap_or(expected)
    }

    fn generic_application_literal_expected_for_mismatch(
        &self,
        allow_generic_literal_display: bool,
        arg_types: &[TypeId],
        args: &[NodeIndex],
        expected: TypeId,
    ) -> Option<TypeId> {
        if !allow_generic_literal_display {
            return None;
        }
        let display_expected = self
            .ctx
            .types
            .get_display_alias(expected)
            .unwrap_or(expected);
        let (base, type_args) = common::application_info(self.ctx.types, display_expected)?;
        if type_args.len() != 1 {
            return None;
        }
        let expected_arg = type_args[0];
        let expected_arg_base = common::widen_literal_type(self.ctx.types, expected_arg);
        if !common::is_primitive_type(self.ctx.types, expected_arg_base) {
            return None;
        }

        let mut candidates = Vec::new();
        let mut seen = FxHashSet::default();
        for candidate in arg_types.iter().copied().chain(
            args.iter()
                .filter_map(|&arg_idx| self.literal_type_from_initializer(arg_idx)),
        ) {
            if common::literal_value(self.ctx.types, candidate).is_some()
                && common::widen_literal_type(self.ctx.types, candidate) == expected_arg_base
                && seen.insert(candidate)
            {
                candidates.push(candidate);
            }
        }
        if candidates.len() < 2 {
            return None;
        }

        let literal_arg = self.ctx.types.factory().union(candidates);
        Some(
            self.ctx
                .types
                .factory()
                .application(base, vec![literal_arg]),
        )
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
        // Only applies when the source is generic and the target is concrete.
        if source_fn.type_params.is_empty() || !target_fn.type_params.is_empty() {
            return None;
        }

        // Check that at least one source type parameter can be mapped from
        // the target's parameter types, confirming these are comparable
        // callable signatures worth building a concrete display target for.
        let tracked_type_params: FxHashSet<_> =
            source_fn.type_params.iter().map(|tp| tp.name).collect();
        let has_mappable_param = source_fn.params.iter().zip(target_fn.params.iter()).any(
            |(source_param, target_param)| {
                let target_type = target_param.type_id;
                if target_type.is_any_unknown_or_error() {
                    return false;
                }
                common::collect_all_types(self.ctx.types, source_param.type_id)
                    .into_iter()
                    .any(|ty| {
                        common::type_param_info(self.ctx.types, ty)
                            .is_some_and(|tp| tracked_type_params.contains(&tp.name))
                    })
            },
        );
        if !has_mappable_param {
            return None;
        }

        // Build a concrete display target using the target's return type.
        // Previously this used the source's return type instantiated with
        // the target's param types, but that produced a target that was
        // trivially assignable from the source (e.g., `(v:string) => string`
        // for `identity<T>(v:T):T` vs `(v:string) => boolean`), suppressing
        // the TS2345 error that tsc emits.
        Some(
            self.ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape {
                    type_params: vec![],
                    params: target_fn.params.clone(),
                    this_type: target_fn.this_type,
                    return_type: target_fn.return_type,
                    type_predicate: target_fn.type_predicate,
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
            callee_has_declared_generic_signature,
            is_super_call,
            is_optional_chain,
            allow_contextual_mismatch_deferral,
            relation_evidence,
            ..
        } = context;
        match result {
            CallResult::Success(return_type) => {
                if is_super_call {
                    return TypeId::VOID;
                }
                self.report_polymorphic_this_indexed_conditional_arg(callee_type, args, arg_types);
                let return_type = self.normalized_builtin_object_entries_return_type(
                    callee_expr,
                    arg_types,
                    return_type,
                );

                self.finalize_call_return_like_success(
                    callee_expr,
                    callee_type,
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
                    // Emit TS2346 when the super() call target has no signatures
                    // (e.g., when the base class is used with invalid type arguments).
                    // Suppress TS2346 when:
                    // - callee type is ERROR (cascading diagnostic)
                    // - callee type is NULL (class extends null; TS17005 covers this)
                    // - callee is a completely empty callable (no sigs, no props) which
                    //   indicates a forward-reference resolution failure (TS2449 covers this)
                    // - the enclosing class extends a forward-referenced class in the
                    //   same file (TS2449 already reported on the heritage clause; tsc
                    //   suppresses the secondary TS2346 in this case).
                    let should_suppress = callee_type == TypeId::ERROR
                        || callee_type == TypeId::NULL
                        || crate::query_boundaries::common::get_callable_shape_for_type(
                            self.ctx.types,
                            callee_type,
                        )
                        .is_some_and(|shape| {
                            shape.call_signatures.is_empty()
                                && shape.construct_signatures.is_empty()
                                && shape.properties.is_empty()
                                && shape.string_index.is_none()
                                && shape.number_index.is_none()
                        })
                        || self.is_super_call_in_forward_referenced_extends(callee_expr);
                    if !should_suppress {
                        self.error_at_node(
                            callee_expr,
                            "Call target does not contain any signatures.",
                            diagnostic_codes::CALL_TARGET_DOES_NOT_CONTAIN_ANY_SIGNATURES,
                        );
                    }
                    return TypeId::VOID;
                }
                if self.is_constructor_type(callee_type)
                    && !self.is_intersection_with_conditional_application(callee_type)
                {
                    self.error_class_constructor_without_new_at(callee_type, callee_expr);
                } else if self.is_get_accessor_call(callee_expr) {
                    self.error_get_accessor_not_callable_at(callee_expr);
                } else if self.ctx.compiler_options.strict_null_checks {
                    let (_non_nullish, nullish_cause) = self.split_nullish_type(callee_type);
                    if let Some(cause) = nullish_cause {
                        self.error_cannot_invoke_possibly_nullish_at(cause, callee_expr);
                    } else if !self.is_in_decorator_expression(callee_expr) {
                        // Don't emit TS2349 for calls inside decorators - decorators
                        // are resolved at runtime and should not be checked for callability.
                        self.error_not_callable_at(callee_type, callee_expr);
                    }
                } else if !self.is_in_decorator_expression(callee_expr) {
                    // Don't emit TS2349 for calls inside decorators - decorators
                    // are resolved at runtime and should not be checked for callability.
                    self.error_not_callable_at(callee_type, callee_expr);
                }
                TypeId::ERROR
            }
            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                // Suppress TS2554/TS2555 for super calls where the parser already
                // emitted TS2754 ("super may not use type arguments") and stripped
                // the type arguments. The resulting `super(args)` call may have the
                // wrong arity because the type-arg stripping changed the resolved
                // constructor shape. TSC's checker handles TS2754 itself and
                // short-circuits before argument checking.
                let suppress_for_super_parse_error =
                    is_super_call && self.node_span_contains_parse_error(call_idx);
                if !self.ctx.has_parse_errors && !suppress_for_super_parse_error {
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
                            // Array literal spreads (e.g., ...[1, 2, 3]) were already
                            // expanded to individual arguments during call checking,
                            // so they have a known count — treat them like tuples.
                            let inner_idx =
                                self.ctx.arena.skip_parenthesized(spread_data.expression);
                            if let Some(expr_node) = self.ctx.arena.get(inner_idx)
                                && expr_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                && self.ctx.arena.get_literal_expr(expr_node).is_some()
                            {
                                return false;
                            }
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
                        callee_type,
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
                let arg_idx = arg_idx.map(|i| self.ctx.arena.skip_parenthesized(i));
                if self
                    .this_argument_satisfies_polymorphic_this_rest_target(arg_idx, actual, expected)
                {
                    return fallback_return;
                }
                if expected == TypeId::NEVER
                    && let Some(return_type) =
                        self.correlated_union_call_recovery_return(callee_type, index, actual)
                {
                    return if fallback_return != TypeId::ERROR {
                        fallback_return
                    } else {
                        return_type
                    };
                }
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
                let aggregate_literal_actual = if self
                    .format_type_diagnostic(expected)
                    .contains("<unknown>")
                {
                    None
                } else {
                    self.literalized_aggregate_actual_for_call_args(args, index, actual, expected)
                };
                let original_is_spread_marker = arg_types.get(index).is_some_and(|&ty| {
                    common::is_spread_marker_tuple(self.ctx.types.as_type_database(), ty)
                });
                let aggregate_rest_mismatch = (common::tuple_elements(self.ctx.types, actual)
                    .is_some()
                    || original_is_spread_marker)
                    && arg_types
                        .get(index)
                        .copied()
                        .is_none_or(|original| original != actual);
                let mut reported_actual = match arg_types.get(index).copied() {
                    Some(TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) | None => actual,
                    Some(original) if self.is_spread_argument_marker_type(original) => actual,
                    Some(original)
                        if original != actual
                            && common::tuple_elements(self.ctx.types, actual).is_some() =>
                    {
                        aggregate_literal_actual.unwrap_or(actual)
                    }
                    Some(original) => original,
                };
                let aggregate_anchor_override = if aggregate_rest_mismatch {
                    self.declared_rest_parameter_index_for_call(callee_expr)
                        .and_then(|rest_index| {
                            self.aggregate_actual_after_declared_rest_start(
                                reported_actual,
                                index,
                                rest_index,
                            )
                            .map(|adjusted| {
                                reported_actual = adjusted;
                                args.get(rest_index).copied().unwrap_or(call_idx)
                            })
                        })
                } else {
                    None
                };
                let polymorphic_this_expected = self.polymorphic_this_indexed_conditional_target(
                    callee_type,
                    args,
                    arg_types,
                    index,
                );
                let preserve_type_parameter_expected_display =
                    common::contains_type_parameters(self.ctx.types, expected);
                let reported_expected = if let Some(expected) = polymorphic_this_expected {
                    expected
                } else if common::contains_this_type(self.ctx.types, expected) {
                    expected
                } else {
                    let reported_expected = self
                        .generic_callable_mismatch_display_target(actual, expected)
                        .unwrap_or(expected);
                    self.preferred_literal_expected_for_mismatch(
                        callee_has_declared_generic_signature,
                        arg_types,
                        args,
                        reported_actual,
                        index,
                        reported_expected,
                    )
                };
                let mut elaborated = false;
                let should_try_deferred_elaboration = self
                    .should_attempt_deferred_literal_elaboration(expected)
                    || arg_idx
                        .is_some_and(|arg_idx| self.argument_supports_literal_elaboration(arg_idx));
                if let Some(arg_idx) = arg_idx {
                    self.suppress_later_call_excess_property_diagnostics(args, arg_idx);
                    let arg_is_object_literal = self.ctx.arena.get(arg_idx).is_some_and(|node| {
                        node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    });
                    let evaluated_expected = self.evaluate_type_with_env(expected);
                    if arg_is_object_literal
                        && (common::type_is_conditional_type_result_with_unresolved_inference(
                            self.ctx.types,
                            expected,
                        ) || common::type_is_conditional_type_result_with_unresolved_inference(
                            self.ctx.types,
                            evaluated_expected,
                        ))
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    // When a callback has a block body, TSC reports TS2345 at the
                    // argument level rather than elaborating with an inner TS2322
                    // on return statements. Compute this BEFORE the elaboration
                    // call so we can skip callback return elaboration entirely.
                    let prefer_argument_level_return_mismatch =
                        self.callback_prefers_argument_level_return_mismatch(arg_idx);
                    let suppress_inner_elaboration =
                        self.callback_has_explicit_param_type_conflict(arg_idx, expected);
                    // Skip elaboration when the original parameter type was a type parameter
                    // (excess properties are allowed for generic calls with type param targets).
                    let skip_for_generic = self
                        .ctx
                        .generic_excess_skip
                        .as_ref()
                        .is_some_and(|skip| index < skip.len() && skip[index]);
                    if should_try_deferred_elaboration
                        && !prefer_argument_level_return_mismatch
                        && !skip_for_generic
                        && !self.should_suppress_weak_key_arg_mismatch(
                            callee_expr,
                            args,
                            index,
                            actual,
                        )
                    {
                        elaborated = self.try_elaborate_object_literal_arg_error_with_source(
                            arg_idx,
                            expected,
                            Some(actual),
                        );
                    }
                    // When a callback has explicitly-typed parameters that conflict with the
                    // expected parameter types, TSC reports TS2345 at the argument level
                    // rather than elaborating with an inner TS2322. Only suppress inner
                    // elaboration when the *parameter* types are the source of the mismatch.
                    if !elaborated
                        && !suppress_inner_elaboration
                        && !prefer_argument_level_return_mismatch
                        && self
                            .callback_body_spans(arg_idx)
                            .iter()
                            .any(|(start, end)| {
                                self.has_diagnostic_code_within_span(
                                    *start,
                                    *end,
                                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                )
                            })
                    {
                        elaborated = true;
                    }
                    // Check stored return-type errors that were pruned by the
                    // arg collection filter. If found, restore the diagnostic
                    // and suppress the outer TS2345.
                    if !elaborated
                        && !suppress_inner_elaboration
                        && !prefer_argument_level_return_mismatch
                    {
                        let stored: Vec<_> = self
                            .ctx
                            .callback_return_type_errors
                            .iter()
                            .filter(|d| {
                                self.callback_body_spans(arg_idx).iter().any(
                                    |(body_start, body_end)| {
                                        d.start >= *body_start && d.start < *body_end
                                    },
                                )
                            })
                            .cloned()
                            .collect();
                        if !stored.is_empty() {
                            self.ctx.diagnostics.extend(stored);
                            elaborated = true;
                        }
                    }
                    // When suppressing inner elaboration, remove any TS2322 inside the
                    // callback body that was left from the arg collection pass, so the
                    // outer TS2345 is the only diagnostic at the argument site.
                    if suppress_inner_elaboration || prefer_argument_level_return_mismatch {
                        let body_spans = self.callback_body_spans(arg_idx);
                        let arg_span = self.callback_argument_span(arg_idx);
                        self.ctx.diagnostics.retain(|d| {
                            !(d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                                && (body_spans.iter().any(|(body_start, body_end)| {
                                    d.start >= *body_start && d.start < *body_end
                                }) || (prefer_argument_level_return_mismatch
                                    && arg_span.is_some_and(|(arg_start, arg_end)| {
                                        d.start >= arg_start && d.start < arg_end
                                    }))))
                        });
                        self.ctx.rebuild_emitted_diagnostics_from_current();
                    }
                    if !elaborated
                        && !suppress_inner_elaboration
                        && allow_contextual_mismatch_deferral
                        && self.should_defer_contextual_argument_mismatch(actual, expected)
                    {
                        return if fallback_return != TypeId::ERROR {
                            fallback_return
                        } else {
                            TypeId::ERROR
                        };
                    }
                    let suppress_weak = self.should_suppress_weak_key_arg_mismatch(
                        callee_expr,
                        args,
                        index,
                        actual,
                    );
                    let resolved_reported_actual = self.resolve_lazy_type(reported_actual);
                    let evaluated_reported_expected =
                        self.evaluate_type_with_env(reported_expected);
                    let suppress_correlated_index_access_never_mismatch = (reported_expected
                        == TypeId::NEVER
                        || evaluated_reported_expected == TypeId::NEVER)
                        && common::index_access_parts(self.ctx.types, reported_actual)
                            .or_else(|| {
                                common::index_access_parts(self.ctx.types, resolved_reported_actual)
                            })
                            .is_some_and(|(_, index)| {
                                common::contains_type_parameters(self.ctx.types, index)
                                    || common::is_type_parameter_like(self.ctx.types, index)
                                    || common::type_param_info(self.ctx.types, index).is_some()
                            });
                    if !suppress_weak
                        && !elaborated
                        && !suppress_correlated_index_access_never_mismatch
                    {
                        let spread_rest_tuple_display = (!aggregate_rest_mismatch)
                            .then(|| {
                                self.spread_rest_tuple_diagnostic_types(arg_idx, reported_expected)
                            })
                            .flatten();
                        if let Some(polymorphic_this_expected) = polymorphic_this_expected {
                            self.error_argument_not_assignable_preserving_param_display(
                                reported_actual,
                                polymorphic_this_expected,
                                arg_idx,
                            );
                        } else if let Some((spread_actual, spread_expected)) =
                            spread_rest_tuple_display
                        {
                            self.error_argument_not_assignable_at(
                                spread_actual,
                                spread_expected,
                                arg_idx,
                            );
                        } else if prefer_argument_level_return_mismatch || aggregate_rest_mismatch {
                            self.error_argument_not_assignable_at(
                                reported_actual,
                                reported_expected,
                                aggregate_anchor_override.unwrap_or(arg_idx),
                            );
                        } else if preserve_type_parameter_expected_display {
                            self.error_argument_not_assignable_preserving_param_display(
                                reported_actual,
                                reported_expected,
                                arg_idx,
                            );
                        } else {
                            let _ = self.report_argument_assignability_with_evidence(
                                relation_evidence,
                                reported_actual,
                                reported_expected,
                                arg_idx,
                            );
                        }
                    }
                } else if index >= arg_types.len() {
                    if should_try_deferred_elaboration
                        && !self.should_suppress_weak_key_arg_mismatch(
                            callee_expr,
                            args,
                            index,
                            actual,
                        )
                        && let Some(last_arg) = args.last().copied()
                    {
                        elaborated = self.try_elaborate_object_literal_arg_error_with_source(
                            last_arg,
                            expected,
                            Some(actual),
                        );
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
                        if aggregate_rest_mismatch {
                            self.error_argument_not_assignable_at(
                                reported_actual,
                                reported_expected,
                                aggregate_anchor_override.unwrap_or(call_idx),
                            );
                        } else {
                            let _ = self.report_argument_assignability_with_evidence(
                                relation_evidence,
                                reported_actual,
                                reported_expected,
                                call_idx,
                            );
                        }
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
                        elaborated = self.try_elaborate_object_literal_arg_error_with_source(
                            last_arg,
                            expected,
                            Some(actual),
                        );
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
                        if aggregate_rest_mismatch {
                            self.error_argument_not_assignable_at(
                                reported_actual,
                                reported_expected,
                                aggregate_anchor_override.unwrap_or(last_arg),
                            );
                        } else {
                            let _ = self.report_argument_assignability_with_evidence(
                                relation_evidence,
                                reported_actual,
                                reported_expected,
                                last_arg,
                            );
                        }
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
                    if aggregate_rest_mismatch {
                        self.error_argument_not_assignable_at(
                            reported_actual,
                            reported_expected,
                            aggregate_anchor_override.unwrap_or(call_idx),
                        );
                    } else {
                        let _ = self.report_argument_assignability_with_evidence(
                            relation_evidence,
                            reported_actual,
                            reported_expected,
                            call_idx,
                        );
                    }
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
                // For regular function calls with arguments, report as TS2345
                // ("Argument of type X is not assignable to parameter of type Y")
                // at the argument position. tsc treats type parameter constraint
                // violations from regular arguments as TS2345, not TS2322.
                // We emit directly (bypassing check_argument_assignable_or_report)
                // because the solver has already confirmed the constraint violation
                // and the checker's re-check may disagree due to different context.
                if !args.is_empty() {
                    self.error_argument_not_assignable_at(inferred_type, constraint_type, args[0]);
                } else {
                    let _ = self.check_assignable_or_report_generic_at(
                        inferred_type,
                        constraint_type,
                        call_idx,
                        call_idx,
                    );
                }
                return_type
            }
            CallResult::NoOverloadMatch {
                failures,
                fallback_return,
                ..
            } => {
                self.ctx.no_overload_call_nodes.insert(call_idx.0);
                let overload_failures_disagree = failures.len() > 1
                    && failures.windows(2).any(|pair| {
                        pair[0].code != pair[1].code
                            || format!("{:?}", pair[0].args) != format!("{:?}", pair[1].args)
                    });
                let has_error_surface = callee_type == TypeId::ERROR
                    || args
                        .iter()
                        .copied()
                        .any(|arg_idx| self.get_type_of_node(arg_idx) == TypeId::ERROR);
                if has_error_surface {
                    return TypeId::ERROR;
                }

                // Check if we should suppress TS2769 due to structural errors on the callee
                let suppress_due_to_structural_errors =
                    self.should_suppress_no_overload_due_to_structural_errors(callee_expr);
                let suppress_due_to_callback_body_errors =
                    self.should_suppress_no_overload_due_to_callback_body_errors(args);

                let should_emit_no_overload_error = !suppress_due_to_structural_errors
                    && !suppress_due_to_callback_body_errors
                    && !self.should_suppress_weak_key_no_overload(callee_expr, args);

                if should_emit_no_overload_error {
                    self.error_no_overload_matches_at(call_idx, &failures);
                }
                let overloaded_callee_has_type_params =
                    common::callable_shape_for_type(self.ctx.types, callee_type).is_some_and(
                        |shape| {
                            shape
                                .call_signatures
                                .iter()
                                .any(|sig| !sig.type_params.is_empty())
                        },
                    );
                let call_is_typed_variable_initializer =
                    self.ctx
                        .arena
                        .get_extended(call_idx)
                        .map(|info| info.parent)
                        .and_then(|parent_idx| {
                            self.ctx
                                .arena
                                .get(parent_idx)
                                .map(|node| (parent_idx, node))
                        })
                        .is_some_and(|(_parent_idx, parent)| {
                            parent.kind == syntax_kind_ext::VARIABLE_DECLARATION
                                && self.ctx.arena.get_variable_declaration(parent).is_some_and(
                                    |decl| {
                                        decl.initializer == call_idx
                                            && decl.type_annotation.is_some()
                                    },
                                )
                        });
                let callee_is_object_assign = self
                    .ctx
                    .arena
                    .get(callee_expr)
                    .and_then(|node| self.ctx.arena.get_access_expr(node))
                    .is_some_and(|access| {
                        self.ctx.arena.get_identifier_text(access.expression) == Some("Object")
                            && self.ctx.arena.get_identifier_text(access.name_or_argument)
                                == Some("assign")
                    });
                if overload_failures_disagree
                    && should_emit_no_overload_error
                    && !overloaded_callee_has_type_params
                    && !call_is_typed_variable_initializer
                    && !callee_is_object_assign
                    && !common::contains_type_parameters(self.ctx.types, fallback_return)
                {
                    TypeId::NEVER
                } else {
                    fallback_return
                }
            }
            CallResult::ThisTypeMismatch {
                expected_this,
                actual_this,
                emit_not_callable,
            } => {
                if emit_not_callable {
                    self.error_not_callable_at(callee_type, callee_expr);
                }
                self.error_this_type_mismatch_at(expected_this, actual_this, callee_expr);
                TypeId::ERROR
            }
        }
    }

    pub(crate) fn should_defer_contextual_argument_mismatch(
        &mut self,
        actual: TypeId,
        expected: TypeId,
    ) -> bool {
        if self.call_target_generic_rest_requires_fixed_arity_error(actual, expected) {
            return false;
        }
        if common::contains_this_type(self.ctx.types, expected) {
            return false;
        }
        // Bare __infer_N expected + concrete actual: inference is done, mismatch is definitive.
        if common::is_bare_infer_placeholder(self.ctx.types, expected)
            && !assign_query::contains_infer_types(self.ctx.types, actual)
            && actual != expected
        {
            return false;
        }
        // When both types are Applications of the same base (e.g., F<CP> vs F<unknown>),
        // the mismatch comes from variance checking, not from contextual typing.
        // Don't defer — the variance rejection is definitive. This matches tsc which
        // reports TS2345 immediately for same-generic-type argument mismatches.
        if let Some(s_app_id) =
            crate::query_boundaries::common::application_id(self.ctx.types, actual)
            && let Some(t_app_id) =
                crate::query_boundaries::common::application_id(self.ctx.types, expected)
        {
            let s_app = self.ctx.types.type_application(s_app_id);
            let t_app = self.ctx.types.type_application(t_app_id);
            if s_app.base == t_app.base
                && !assign_query::contains_infer_types(self.ctx.types, actual)
                && !assign_query::contains_infer_types(self.ctx.types, expected)
                && !assign_query::contains_type_parameters(self.ctx.types, actual)
                && !assign_query::contains_type_parameters(self.ctx.types, expected)
            {
                return false;
            }
        }
        let has_callable_shape = |this: &mut Self, ty: TypeId| {
            if crate::query_boundaries::common::function_shape_for_type(this.ctx.types, ty)
                .is_some()
            {
                return true;
            }
            if common::callable_shape_for_type(this.ctx.types, ty).is_some() {
                return true;
            }
            let evaluated = this.evaluate_type_with_env(ty);
            crate::query_boundaries::common::function_shape_for_type(this.ctx.types, evaluated)
                .is_some()
                || common::callable_shape_for_type(this.ctx.types, evaluated).is_some()
        };
        let callable_mismatch =
            has_callable_shape(self, actual) && has_callable_shape(self, expected);
        let actual_has_generic_signatures = self.callable_has_own_generic_signatures(actual);
        let expected_has_generic_signatures = self.callable_has_own_generic_signatures(expected);
        let has_construct_signatures = |this: &mut Self, ty: TypeId| {
            common::callable_shape_for_type(this.ctx.types, ty)
                .or_else(|| {
                    let evaluated = this.evaluate_type_with_env(ty);
                    common::callable_shape_for_type(this.ctx.types, evaluated)
                })
                .is_some_and(|shape| !shape.construct_signatures.is_empty())
        };
        let constructor_mismatch =
            has_construct_signatures(self, actual) && has_construct_signatures(self, expected);
        let constructor_generic_mismatch = constructor_mismatch
            && (actual_has_generic_signatures || expected_has_generic_signatures);
        let actual_contains_infer = assign_query::contains_infer_types(self.ctx.types, actual);
        let expected_contains_infer = assign_query::contains_infer_types(self.ctx.types, expected);
        if actual_contains_infer || expected_contains_infer {
            let evaluated_actual = self.evaluate_type_with_env(actual);
            let evaluated_expected = self.evaluate_type_with_env(expected);
            let evaluated_still_has_holes =
                assign_query::contains_infer_types(self.ctx.types, evaluated_actual)
                    || assign_query::contains_infer_types(self.ctx.types, evaluated_expected)
                    || assign_query::contains_type_parameters(self.ctx.types, evaluated_actual)
                    || assign_query::contains_type_parameters(self.ctx.types, evaluated_expected);
            return evaluated_still_has_holes;
        }
        if callable_mismatch {
            let refined_actual = if self
                .target_has_concrete_return_context_for_generic_refinement(expected)
            {
                self.instantiate_generic_function_argument_against_target_for_refinement(
                    actual, expected,
                )
            } else {
                self.instantiate_generic_function_argument_against_target_params(actual, expected)
            };
            let refined_actual = self.normalize_contextual_signature_with_env(refined_actual);
            let refined_expected = self.normalize_contextual_signature_with_env(expected);
            let refined_still_has_holes =
                assign_query::contains_infer_types(self.ctx.types, refined_actual)
                    || assign_query::contains_infer_types(self.ctx.types, refined_expected)
                    || assign_query::contains_type_parameters(self.ctx.types, refined_actual)
                    || assign_query::contains_type_parameters(self.ctx.types, refined_expected);
            if constructor_generic_mismatch {
                return !self
                    .generic_constructor_mismatch_has_uncovered_required_arity(actual, expected);
            }
            if !refined_still_has_holes {
                return false;
            }
            // Defer only when holes are in expected (outer inference will resolve them),
            // not when holes are in actual (those are permanent outer-scope type params).
            if !actual_has_generic_signatures && !expected_has_generic_signatures {
                let actual_has_holes =
                    assign_query::contains_infer_types(self.ctx.types, refined_actual)
                        || assign_query::contains_type_parameters(self.ctx.types, refined_actual);
                if !actual_has_holes {
                    return true;
                }
                let actual_type_params: rustc_hash::FxHashSet<_> =
                    common::collect_referenced_types(self.ctx.types, refined_actual)
                        .into_iter()
                        .filter(|&ty| common::type_param_info(self.ctx.types, ty).is_some())
                        .collect();
                let expected_type_params: rustc_hash::FxHashSet<_> =
                    common::collect_referenced_types(self.ctx.types, refined_expected)
                        .into_iter()
                        .filter(|&ty| common::type_param_info(self.ctx.types, ty).is_some())
                        .collect();
                if !actual_type_params.is_empty()
                    && actual_type_params
                        .iter()
                        .all(|ty| expected_type_params.contains(ty))
                {
                    return true;
                }
            }
        }
        // Defer callable mismatches only when a callable has its own generic signatures
        // (higher-order inference may still resolve them), not for outer-scope type params.
        if callable_mismatch && (actual_has_generic_signatures || expected_has_generic_signatures) {
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
            // Don't defer when the base types of generic instantiations are different
            // classes. For example, B<T> vs A<T> where A has private members should
            // NOT be deferred — the mismatch is structural and type parameter resolution
            // cannot fix it. Only defer when the types could plausibly become compatible
            // once type parameters are resolved.
            if self.are_incompatible_generic_class_instances(actual, expected) {
                return false;
            }
            // When both sides are *bare* `TypeParameter` types with different
            // identities, neither side is in flight. Distinct enclosing-scope
            // type parameters never become equal under inference, so the
            // solver's rejection is permanent — deferring would silently drop
            // a real TS2345. Mirrors `(x: T) => void` vs `(x: U) => void` for
            // non-callable bare type parameters.
            //
            // `Infer` types are excluded so in-flight `infer T` placeholders
            // inside conditional inference still defer.
            if actual != expected
                && crate::query_boundaries::checkers::generic::is_bare_named_type_parameter(
                    self.ctx.types,
                    actual,
                )
                && crate::query_boundaries::checkers::generic::is_bare_named_type_parameter(
                    self.ctx.types,
                    expected,
                )
            {
                return false;
            }
            return true;
        }
        assign_query::is_any_type(self.ctx.types, expected)
    }

    /// Check if `actual` and `expected` are generic instantiations of different classes.
    ///
    /// When `B<T>` and `A<T>` are Applications of different class definitions,
    /// the mismatch is structural (e.g., different private brands) and cannot be
    /// resolved by type parameter instantiation. In this case, deferral is incorrect.
    fn are_incompatible_generic_class_instances(&self, actual: TypeId, expected: TypeId) -> bool {
        use crate::query_boundaries::common::{application_id, lazy_def_id};

        let db = self.ctx.types;

        // Extract the base DefId from a class Application type (e.g., A<T> -> DefId_A).
        // Type aliases such as Partial<T> remain transparent enough for deferred
        // assignability; only nominal class bases make the rejection permanent.
        let base_def = |ty: TypeId| -> Option<tsz_solver::DefId> {
            let app_id = application_id(db, ty)?;
            let app = db.type_application(app_id);
            let def_id = lazy_def_id(db, app.base)?;
            matches!(
                tsz_solver::TypeResolver::get_def_kind(&self.ctx, def_id),
                Some(tsz_solver::def::DefKind::Class)
            )
            .then_some(def_id)
        };

        let actual_def = base_def(actual);
        let expected_def = base_def(expected);

        match (actual_def, expected_def) {
            (Some(a), Some(e)) => a != e,
            _ => false,
        }
    }

    fn call_target_generic_rest_requires_fixed_arity_error(
        &mut self,
        actual: TypeId,
        expected: TypeId,
    ) -> bool {
        let normalize = |shape: tsz_solver::FunctionShape| {
            let mut normalized = shape.clone();
            normalized.params = shape
                .params
                .iter()
                .flat_map(|param| common::unpack_tuple_rest_parameter(self.ctx.types, param))
                .collect();
            normalized
        };

        let actual = self.normalize_contextual_signature_with_env(actual);
        let expected = self.normalize_contextual_signature_with_env(expected);
        let Some(actual_shape) = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            actual,
        ) else {
            return false;
        };
        let Some(expected_shape) =
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                expected,
            )
        else {
            return false;
        };

        let actual_shape = normalize(actual_shape);
        let expected_shape = normalize(expected_shape);
        let Some(expected_rest) = expected_shape.params.last().filter(|param| param.rest) else {
            return false;
        };

        if !common::is_type_parameter_like(self.ctx.types, expected_rest.type_id)
            && !common::contains_type_parameters(self.ctx.types, expected_rest.type_id)
        {
            return false;
        }

        let actual_required = actual_shape
            .params
            .iter()
            .filter(|param| !param.optional && !param.rest)
            .count();
        let expected_fixed = expected_shape.params.len().saturating_sub(1);
        actual_required > expected_fixed
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
        self.ctx.rebuild_emitted_diagnostics_from_current();
    }

    pub(crate) fn build_expanded_args_for_error(&mut self, args: &[NodeIndex]) -> Vec<NodeIndex> {
        let mut expanded = Vec::with_capacity(args.len());
        for &arg_idx in args {
            if let Some(n) = self.ctx.arena.get(arg_idx)
                && n.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread_expression) = self
                    .ctx
                    .arena
                    .get_spread(n)
                    .map(|spread| spread.expression)
                    .or_else(|| self.ctx.arena.get_children(arg_idx).first().copied())
            {
                let spread_type = self.get_type_of_node(spread_expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);
                let spread_type = self.resolve_lazy_type(spread_type);
                if let Some(elems) =
                    crate::query_boundaries::common::tuple_elements(self.ctx.types, spread_type)
                {
                    expanded.extend(std::iter::repeat_n(arg_idx, elems.len()));
                    continue;
                }
                // Array literal spreads have known element count — expand them
                let inner_idx = self.ctx.arena.skip_parenthesized(spread_expression);
                if let Some(expr_node) = self.ctx.arena.get(inner_idx)
                    && expr_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
                {
                    expanded.extend(std::iter::repeat_n(arg_idx, literal.elements.nodes.len()));
                    continue;
                }
            }
            expanded.push(arg_idx);
        }
        expanded
    }

    /// Check if TS2769 (no overload matches) should be suppressed due to structural
    /// errors on the callee type. When a class/interface has structural errors
    /// (TS2420, TS2430, TS2694), we suppress "no overload matches" errors because
    /// the type is known to be broken and the primary errors should be shown instead.
    fn should_suppress_no_overload_due_to_structural_errors(
        &mut self,
        callee_expr: NodeIndex,
    ) -> bool {
        // Only check for property access expressions (e.g., Promise.try)
        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };

        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };

        // Get the base expression (e.g., Promise in Promise.try)
        let base_expr = access.expression;

        // Resolve the base identifier to its symbol
        let Some(symbol_id) = self.resolve_identifier_symbol(base_expr) else {
            return false;
        };

        // Check if this symbol has structural error diagnostics
        self.symbol_has_structural_errors(symbol_id)
    }

    fn should_suppress_no_overload_due_to_callback_body_errors(&self, args: &[NodeIndex]) -> bool {
        const CALLBACK_BODY_DIAGNOSTIC_CODES: &[u32] = &[2322, 2339, 2345, 2347, 7006, 7019, 7031];

        args.iter().copied().any(|arg_idx| {
            self.is_callback_like_argument(arg_idx)
                && self
                    .callback_body_spans(arg_idx)
                    .iter()
                    .any(|(start, end)| {
                        self.ctx.diagnostics.iter().any(|diag| {
                            diag.start >= *start
                                && diag.start < *end
                                && CALLBACK_BODY_DIAGNOSTIC_CODES.contains(&diag.code)
                        })
                    })
        })
    }

    /// Check if a symbol has structural error diagnostics (TS2420, TS2430, TS2694).
    fn symbol_has_structural_errors(&self, symbol_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) else {
            return false;
        };

        let structural_error_codes = [
            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
            diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
            diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
        ];

        // Check if any structural error diagnostics are within this symbol's declaration spans
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let decl_start = node.pos;
            let decl_end = node.end;

            for diag in &self.ctx.diagnostics {
                if structural_error_codes.contains(&diag.code)
                    && diag.start >= decl_start
                    && diag.start < decl_end
                {
                    return true;
                }
            }
        }

        false
    }
}
