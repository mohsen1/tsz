use crate::query_boundaries::class_type as class_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if a type (including intersection members) has generic construct signatures.
    ///
    /// For intersection types like `T & Constructor<MyMixin>`, checks each member
    /// for generic construct signatures. This is needed because `construct_signatures_for_type`
    /// only works on direct `Callable` types, not intersections.
    pub(crate) fn has_generic_construct_signatures(&self, type_id: TypeId) -> bool {
        if let Some(sigs) = class_query::construct_signatures_for_type(self.ctx.types, type_id)
            && sigs.iter().any(|sig| !sig.type_params.is_empty())
        {
            return true;
        }
        // For intersection types, check each member
        if let Some(members) = class_query::intersection_members(self.ctx.types, type_id) {
            for member in &members {
                if self.has_generic_construct_signatures(*member) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn check_heritage_call_expression_constructor_compatibility(
        &mut self,
        expr_idx: NodeIndex,
        base_constructor_type: TypeId,
        explicit_type_args: Option<&tsz_parser::parser::NodeList>,
    ) {
        if self.heritage_call_has_invalid_mixin_constructor_constraint(expr_idx) {
            self.error_at_node(
                expr_idx,
                crate::diagnostics::diagnostic_messages::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
                crate::diagnostics::diagnostic_codes::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
            );
            return;
        }

        let mut signatures = Vec::new();
        self.collect_heritage_call_expression_constructor_signatures(
            base_constructor_type,
            &mut signatures,
            &mut rustc_hash::FxHashSet::default(),
        );
        if signatures.is_empty() {
            return;
        }

        let type_args_nodes: &[NodeIndex] = explicit_type_args
            .map(|args| args.nodes.as_slice())
            .unwrap_or(&[]);
        let provided_count = type_args_nodes.len();

        let provided_types: Vec<TypeId> = type_args_nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        let matching: Vec<&tsz_solver::CallSignature> = signatures
            .iter()
            .filter(|sig| {
                let max = sig.type_params.len();
                let min = sig
                    .type_params
                    .iter()
                    .filter(|tp| tp.default.is_none())
                    .count();
                provided_count >= min && provided_count <= max
            })
            .collect();

        if provided_count > 0 && matching.is_empty() {
            let anchor =
                self.find_heritage_call_expression_type_argument_anchor(expr_idx, type_args_nodes);
            let message = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::NO_BASE_CONSTRUCTOR_HAS_THE_SPECIFIED_NUMBER_OF_TYPE_ARGUMENTS,
                &[],
            );
            self.error(
                anchor,
                1,
                message,
                crate::diagnostics::diagnostic_codes::NO_BASE_CONSTRUCTOR_HAS_THE_SPECIFIED_NUMBER_OF_TYPE_ARGUMENTS,
            );
            return;
        }

        if matching.is_empty() {
            return;
        }

        // When the base constructor type is an intersection (e.g., mixin patterns
        // like `T & (new (...args) => Mixin)`), constructor signatures come from
        // different intersection members and naturally have different return types.
        // tsc doesn't compare return types across intersection members. Invalid
        // mixin constructor constraints are handled by the explicit check above.
        let has_intersection_instance =
            crate::query_boundaries::flow_analysis::instance_type_from_constructor(
                self.ctx.types,
                base_constructor_type,
            )
            .is_some_and(|instance_type| {
                crate::query_boundaries::common::intersection_members(self.ctx.types, instance_type)
                    .is_some()
            });
        let has_prototype_property = crate::query_boundaries::common::has_property_by_str(
            self.ctx.types,
            base_constructor_type,
            "prototype",
        );
        if crate::query_boundaries::common::is_intersection_type(
            self.ctx.types,
            base_constructor_type,
        ) || has_intersection_instance
            || has_prototype_property
        {
            return;
        }

        let mut return_types = Vec::with_capacity(matching.len());
        for sig in matching {
            let mut args = provided_types.clone();
            if args.len() < sig.type_params.len() {
                for param in sig.type_params.iter().skip(args.len()) {
                    let fallback = param
                        .default
                        .or(param.constraint)
                        .unwrap_or(TypeId::UNKNOWN);
                    args.push(fallback);
                }
            }
            if args.len() > sig.type_params.len() {
                args.truncate(sig.type_params.len());
            }
            let instantiated = self.instantiate_signature(sig, &args);
            return_types.push(instantiated.return_type);
        }

        let Some((first_return, rest)) = return_types.split_first() else {
            return;
        };
        for &candidate_return in rest {
            if !self.are_mutually_assignable(*first_return, candidate_return)
                || !self.are_mutually_assignable(candidate_return, *first_return)
            {
                self.error_at_node(
                expr_idx,
                crate::diagnostics::diagnostic_messages::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
                crate::diagnostics::diagnostic_codes::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
            );
                return;
            }
        }
    }

    fn collect_heritage_call_expression_constructor_signatures(
        &self,
        type_id: TypeId,
        signatures: &mut Vec<tsz_solver::CallSignature>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        if !visited.insert(type_id) {
            return;
        }

        if let Some(sigs) = class_query::construct_signatures_for_type(self.ctx.types, type_id) {
            signatures.extend(sigs);
        }

        if let Some(members) = class_query::intersection_members(self.ctx.types, type_id) {
            for member in &members {
                self.collect_heritage_call_expression_constructor_signatures(
                    *member, signatures, visited,
                );
            }
        }
    }

    fn find_heritage_call_expression_type_argument_anchor(
        &self,
        expr_idx: NodeIndex,
        type_arg_nodes: &[NodeIndex],
    ) -> u32 {
        let (call_expr_start, _) = self.get_node_span(expr_idx).unwrap_or((0, 0));
        let explicit_start = type_arg_nodes
            .first()
            .and_then(|&arg| self.get_node_span(arg).map(|(start, _)| start));

        find_heritage_call_expression_type_argument_anchor_impl(
            call_expr_start,
            explicit_start,
            call_expr_start,
        )
    }
}

const fn find_heritage_call_expression_type_argument_anchor_impl(
    call_expr_start: u32,
    explicit_type_arg_start: Option<u32>,
    fallback_start: u32,
) -> u32 {
    if explicit_type_arg_start.is_some() {
        call_expr_start
    } else {
        fallback_start
    }
}

#[cfg(test)]
mod tests {
    use super::find_heritage_call_expression_type_argument_anchor_impl;

    #[test]
    fn test_prefers_explicit_type_argument_node_start() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, Some(23), 5);
        assert_eq!(anchor, 15);
    }

    #[test]
    fn test_falls_back_to_call_start_when_source_text_missing() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(26, Some(2), 5);
        assert_eq!(anchor, 26);
    }

    #[test]
    fn test_falls_back_to_call_start_without_type_arguments() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, None, 7);
        assert_eq!(anchor, 7);
    }
}
