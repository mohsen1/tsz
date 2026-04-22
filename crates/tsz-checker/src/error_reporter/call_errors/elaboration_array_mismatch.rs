//! Array-literal mismatch elaboration helpers for call errors.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter::call_errors) fn elaboration_tuple_element_type_at(
        &self,
        elements: &[tsz_solver::TupleElement],
        index: usize,
    ) -> Option<TypeId> {
        if let Some(element) = elements.get(index) {
            if element.rest {
                return crate::query_boundaries::common::array_element_type(
                    self.ctx.types,
                    element.type_id,
                )
                .or(Some(element.type_id));
            }
            return Some(element.type_id);
        }

        let rest = elements.last().filter(|element| element.rest)?;
        crate::query_boundaries::common::array_element_type(self.ctx.types, rest.type_id)
            .or(Some(rest.type_id))
    }

    pub(in crate::error_reporter::call_errors) fn try_elaborate_array_literal_mismatch_from_failure_reason(
        &mut self,
        arg_idx: NodeIndex,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        use crate::query_boundaries::common::SubtypeFailureReason;
        use tsz_parser::parser::syntax_kind_ext;

        if matches!(source_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
            || matches!(target_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
        {
            return false;
        }

        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        if arg_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return false;
        }
        let Some(arr) = self.ctx.arena.get_literal_expr(arg_node).cloned() else {
            return false;
        };
        if self.call_argument_targets_generic_parameter(arg_idx) {
            return false;
        }

        let effective_target_type = self.evaluate_type_with_env(target_type);
        let effective_target_type = self.resolve_type_for_property_access(effective_target_type);
        let effective_target_type = self.resolve_lazy_type(effective_target_type);
        let effective_target_type = self.evaluate_application_type(effective_target_type);
        let ctx_helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            effective_target_type,
            self.ctx.compiler_options.no_implicit_any,
        );
        let tuple_target_elements =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, effective_target_type);

        let analysis = self.analyze_assignability_failure(source_type, target_type);
        match analysis.failure_reason {
            Some(SubtypeFailureReason::TupleElementTypeMismatch {
                index,
                source_element,
                target_element,
            }) => {
                let Some(&elem_idx) = arr.elements.nodes.get(index) else {
                    return false;
                };
                let is_spread = self
                    .ctx
                    .arena
                    .get(elem_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::SPREAD_ELEMENT);
                if is_spread {
                    return false;
                }
                self.error_type_not_assignable_at_with_anchor(
                    source_element,
                    target_element,
                    elem_idx,
                );
                true
            }
            Some(SubtypeFailureReason::TupleElementMismatch { .. }) => {
                for (index, &elem_idx) in arr.elements.nodes.iter().enumerate() {
                    let is_spread = self
                        .ctx
                        .arena
                        .get(elem_idx)
                        .is_some_and(|node| node.kind == syntax_kind_ext::SPREAD_ELEMENT);
                    if is_spread {
                        continue;
                    }

                    let target_element_type =
                        if let Some(elements) = tuple_target_elements.as_deref() {
                            let Some(t) = self.elaboration_tuple_element_type_at(elements, index)
                            else {
                                continue;
                            };
                            t
                        } else if let Some(t) = ctx_helper.get_tuple_element_type(index) {
                            t
                        } else if let Some(t) = ctx_helper.get_array_element_type() {
                            t
                        } else if let Some(t) = crate::query_boundaries::common::array_element_type(
                            self.ctx.types,
                            effective_target_type,
                        ) {
                            t
                        } else {
                            continue;
                        };

                    let elem_type = self.elaboration_source_expression_type(elem_idx);
                    if matches!(elem_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
                        || matches!(
                            target_element_type,
                            TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
                        )
                    {
                        continue;
                    }

                    if !self.is_assignable_to(elem_type, target_element_type) {
                        self.error_type_not_assignable_at_with_anchor(
                            elem_type,
                            target_element_type,
                            elem_idx,
                        );
                        return true;
                    }
                }
                false
            }
            Some(SubtypeFailureReason::ArrayElementMismatch {
                source_element: _,
                target_element,
            }) => {
                for &elem_idx in &arr.elements.nodes {
                    let is_spread = self
                        .ctx
                        .arena
                        .get(elem_idx)
                        .is_some_and(|node| node.kind == syntax_kind_ext::SPREAD_ELEMENT);
                    if is_spread {
                        continue;
                    }
                    let elem_type = self.elaboration_source_expression_type(elem_idx);
                    if matches!(elem_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
                        continue;
                    }
                    if !self.is_assignable_to(elem_type, target_element) {
                        self.error_type_not_assignable_at_with_anchor(
                            elem_type,
                            target_element,
                            elem_idx,
                        );
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    pub(in crate::error_reporter::call_errors) fn call_argument_targets_generic_parameter(
        &mut self,
        arg_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = arg_idx;
        while current.is_some() {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.ctx.arena.get_call_expr(node)
                && let Some(args) = &call.arguments
                && let Some(arg_pos) = args
                    .nodes
                    .iter()
                    .position(|&candidate| candidate == arg_idx)
            {
                if call
                    .type_arguments
                    .as_ref()
                    .is_some_and(|type_args| !type_args.nodes.is_empty())
                {
                    return false;
                }
                let callee_type = self.get_type_of_node(call.expression);
                let arg_count = args.nodes.len();

                let raw_param_contains_type_params = |sig: &tsz_solver::CallSignature| {
                    if !self.call_signature_accepts_arg_count(sig, arg_count) {
                        return false;
                    }
                    self.raw_param_for_argument_index(sig, arg_pos)
                        .is_some_and(|raw_param| {
                            crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                raw_param.type_id,
                            ) || crate::query_boundaries::common::contains_infer_types(
                                self.ctx.types,
                                raw_param.type_id,
                            )
                        })
                };

                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    callee_type,
                ) {
                    let sig = tsz_solver::CallSignature {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: shape.this_type,
                        return_type: shape.return_type,
                        type_predicate: shape.type_predicate,
                        is_method: shape.is_method,
                    };
                    if raw_param_contains_type_params(&sig) {
                        return true;
                    }
                }

                if let Some(signatures) = crate::query_boundaries::common::call_signatures_for_type(
                    self.ctx.types,
                    callee_type,
                ) {
                    for sig in signatures {
                        if raw_param_contains_type_params(&sig) {
                            return true;
                        }
                    }
                }

                return false;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        false
    }
}
