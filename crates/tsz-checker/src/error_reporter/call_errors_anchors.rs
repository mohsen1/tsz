//! Anchor resolution helpers for call error diagnostics.
//!
//! These helpers locate the precise AST node to anchor a diagnostic on
//! when reporting overload and literal argument mismatch errors.
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn first_call_argument_anchor(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;
        call.arguments.as_ref()?.nodes.first().copied()
    }

    /// Returns `true` if the call expression at `idx` has exactly one argument.
    pub(super) fn call_has_single_argument(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        call.arguments
            .as_ref()
            .map_or(false, |args| args.nodes.len() == 1)
    }

    pub(super) fn overload_callee_is_property_like(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };

        matches!(
            callee_node.kind,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        )
    }

    pub(super) fn overload_literal_argument_anchor(
        &mut self,
        idx: NodeIndex,
        failures: &[tsz_solver::PendingDiagnostic],
    ) -> Option<NodeIndex> {
        use crate::diagnostics::diagnostic_codes;

        if failures.is_empty()
            || !failures.iter().all(|failure| {
                failure.code
                    == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            })
        {
            return None;
        }

        let arg_idx = self.first_call_argument_anchor(idx)?;
        let mut shared_anchor = None;

        for failure in failures {
            let expected_type = match failure.args.as_slice() {
                [_, tsz_solver::DiagnosticArg::Type(expected_type)] => *expected_type,
                _ => return None,
            };
            let anchor = self.literal_argument_mismatch_anchor(arg_idx, expected_type)?;
            if let Some(existing) = shared_anchor {
                if existing != anchor {
                    return None;
                }
            } else {
                shared_anchor = Some(anchor);
            }
        }

        shared_anchor
    }

    pub(super) fn shared_overload_argument_anchor(
        &mut self,
        idx: NodeIndex,
        failures: &[&tsz_solver::PendingDiagnostic],
    ) -> Option<NodeIndex> {
        use crate::diagnostics::diagnostic_codes;

        let node = self.ctx.arena.get(idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;
        let args = call.arguments.as_ref()?;

        let mut shared = None;
        for failure in failures {
            if failure.code
                != diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            {
                return None;
            }

            let (actual_type, expected_type) = match failure.args.as_slice() {
                [
                    tsz_solver::DiagnosticArg::Type(actual_type),
                    tsz_solver::DiagnosticArg::Type(expected_type),
                ] => (*actual_type, *expected_type),
                _ => return None,
            };

            let mut matching_args = Vec::new();
            for &arg_idx in &args.nodes {
                let arg_type = self.get_type_of_node(arg_idx);
                let matches_actual = arg_type == actual_type
                    || self.resolve_lazy_type(arg_type) == actual_type
                    || self.resolve_lazy_type(actual_type) == arg_type;
                let mismatches_expected = expected_type != TypeId::ERROR
                    && expected_type != TypeId::UNKNOWN
                    && !self.is_assignable_to(arg_type, expected_type);

                if matches_actual || mismatches_expected {
                    matching_args.push(arg_idx);
                }
            }

            let [anchor_idx] = matching_args.as_slice() else {
                return None;
            };
            if let Some(existing) = shared {
                if existing != *anchor_idx {
                    return None;
                }
            } else {
                shared = Some(*anchor_idx);
            }
        }

        shared
    }

    pub(super) fn literal_argument_mismatch_anchor(
        &mut self,
        source_idx: NodeIndex,
        target_type: TypeId,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
        let node = self.ctx.arena.get(expr_idx)?;

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.object_literal_mismatch_anchor(expr_idx, target_type)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.array_literal_mismatch_anchor(expr_idx, target_type)
            }
            _ => None,
        }
    }

    fn object_literal_mismatch_anchor(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_solver::SubtypeFailureReason;

        let source_type = self.get_type_of_node(arg_idx);
        let effective_param_type = if let (Some(non_nullish), Some(_nullish_cause)) =
            self.split_nullish_type(param_type)
        {
            non_nullish
        } else {
            param_type
        };
        if effective_param_type == TypeId::NEVER {
            return None;
        }

        let arg_node = self.ctx.arena.get(arg_idx)?;
        let obj = self.ctx.arena.get_literal_expr(arg_node)?.clone();

        for &elem_idx in &obj.elements.nodes {
            let elem_node = match self.ctx.arena.get(elem_idx) {
                Some(node) => node,
                None => continue,
            };

            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                    (prop.name, prop.initializer)
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(elem_node)?;
                    (prop.name, prop.name)
                }
                _ => continue,
            };

            let prop_name = self.object_literal_property_name_text(prop_name_idx)?;
            let (target_prop_type, _) = self.object_literal_target_property_type(
                effective_param_type,
                prop_name_idx,
                &prop_name,
            )?;
            let source_prop_type = self.get_type_of_node(prop_value_idx);

            if source_prop_type == TypeId::ERROR
                || source_prop_type == TypeId::ANY
                || target_prop_type == TypeId::ERROR
                || target_prop_type == TypeId::ANY
            {
                continue;
            }

            if !self.is_assignable_to(source_prop_type, target_prop_type) {
                return self
                    .literal_argument_mismatch_anchor(prop_value_idx, target_prop_type)
                    .or(Some(prop_name_idx));
            }
        }

        let analysis = self.analyze_assignability_failure(source_type, effective_param_type);
        match analysis.failure_reason.as_ref() {
            Some(
                SubtypeFailureReason::MissingProperty { .. }
                | SubtypeFailureReason::MissingProperties { .. }
                | SubtypeFailureReason::OptionalPropertyRequired { .. },
            ) => Some(arg_idx),
            _ => None,
        }
    }

    fn array_literal_mismatch_anchor(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        if param_type == TypeId::NEVER {
            return None;
        }

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return None,
        };
        let arr = self.ctx.arena.get_literal_expr(arg_node)?.clone();
        let ctx_helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            param_type,
            self.ctx.compiler_options.no_implicit_any,
        );

        for (index, &elem_idx) in arr.elements.nodes.iter().enumerate() {
            let elem_node = match self.ctx.arena.get(elem_idx) {
                Some(node) => node,
                None => continue,
            };
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                continue;
            }

            let target_element_type = if let Some(t) = ctx_helper.get_tuple_element_type(index) {
                t
            } else if let Some(t) = ctx_helper.get_array_element_type() {
                t
            } else {
                continue;
            };

            if let Some(anchor) =
                self.literal_argument_mismatch_anchor(elem_idx, target_element_type)
            {
                return Some(anchor);
            }

            let elem_type = self.elaboration_source_expression_type(elem_idx);
            if elem_type == TypeId::ERROR
                || elem_type == TypeId::ANY
                || target_element_type == TypeId::ERROR
                || target_element_type == TypeId::ANY
            {
                continue;
            }

            if !self.is_assignable_to(elem_type, target_element_type) {
                return Some(elem_idx);
            }
        }

        None
    }

    pub(super) fn is_concat_call(&self, expr: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        self.ctx
            .arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "concat")
    }

    pub(super) fn should_suppress_concat_overload_error(&mut self, idx: NodeIndex) -> bool {
        use crate::query_boundaries::checkers::call::array_element_type_for_type;
        use crate::query_boundaries::common::contains_type_parameters;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if name_ident.escaped_text != "concat" {
            return false;
        }

        let Some(args) = &call.arguments else {
            return false;
        };
        if args.nodes.is_empty() {
            return false;
        }

        args.nodes.iter().all(|&arg_idx| {
            let arg_type = self.get_type_of_node(arg_idx);
            array_element_type_for_type(self.ctx.types, arg_type).is_some()
                && contains_type_parameters(self.ctx.types, arg_type)
        })
    }
}
