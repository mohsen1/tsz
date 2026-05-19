use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeList;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn function_type_predicate_text(
        &self,
        func_type_id: tsz_solver::types::TypeId,
        outer_type_params: Option<&NodeList>,
    ) -> Option<String> {
        let interner = self.type_interner?;
        let signature =
            tsz_solver::type_queries::flow::extract_predicate_signature(interner, func_type_id)?;
        let predicate = signature.predicate;

        let mut text = String::new();
        if predicate.asserts {
            text.push_str("asserts ");
        }
        match predicate.target {
            tsz_solver::types::TypePredicateTarget::This => text.push_str("this"),
            tsz_solver::types::TypePredicateTarget::Identifier(atom) => {
                text.push_str(&interner.resolve_atom(atom));
            }
        }
        if let Some(type_id) = predicate.type_id {
            text.push_str(" is ");
            let type_text = self
                .type_parameter_strict_null_predicate_text(type_id, outer_type_params)
                .unwrap_or_else(|| {
                    outer_type_params
                        .filter(|type_params| !type_params.nodes.is_empty())
                        .map(|type_params| {
                            self.print_type_id_with_outer_type_params(type_id, type_params)
                        })
                        .unwrap_or_else(|| self.print_type_id_for_inferred_declaration(type_id))
                });
            text.push_str(&type_text);
        }
        Some(text)
    }

    fn type_parameter_strict_null_predicate_text(
        &self,
        type_id: tsz_solver::types::TypeId,
        outer_type_params: Option<&NodeList>,
    ) -> Option<String> {
        let interner = self.type_interner?;
        let is_type_param_like = |id| tsz_solver::visitor::type_param_info(interner, id).is_some();
        // Returns (type_param, other_side) so callers don't re-check which side is the param.
        let find_type_param = |a: tsz_solver::types::TypeId, b: tsz_solver::types::TypeId| {
            if is_type_param_like(a) {
                Some((a, b))
            } else if is_type_param_like(b) {
                Some((b, a))
            } else {
                None
            }
        };

        // Pattern 1: intersection T & ({} | undefined)
        if let Some(members) = tsz_solver::type_queries::get_intersection_members(interner, type_id)
            && let [left, right] = members.as_slice()
            && let Some((type_param, union_type)) = find_type_param(*left, *right)
        {
            if let Some(union_members) =
                tsz_solver::type_queries::get_union_members(interner, union_type)
                && union_members.contains(&tsz_solver::types::TypeId::UNDEFINED)
                && union_members
                    .iter()
                    .copied()
                    .any(|m| tsz_solver::type_queries::is_empty_object_type(interner, m))
            {
                return Some(
                    self.format_type_param_strict_null_predicate(type_param, outer_type_params),
                );
            }
        }

        // Pattern 2: union [(T & undefined), X] where X is a narrowed form of T.
        // The solver may distribute T & ({} | undefined) as (T & undefined) | NonNullable<T>.
        if let Some(union_members) = tsz_solver::type_queries::get_union_members(interner, type_id)
            && union_members.len() == 2
        {
            for &candidate in union_members.iter() {
                if let Some(int_members) =
                    tsz_solver::type_queries::get_intersection_members(interner, candidate)
                    && let [a, b] = int_members.as_slice()
                    && (*a == tsz_solver::types::TypeId::UNDEFINED
                        || *b == tsz_solver::types::TypeId::UNDEFINED)
                    && let Some((type_param, _)) = find_type_param(*a, *b)
                {
                    return Some(
                        self.format_type_param_strict_null_predicate(type_param, outer_type_params),
                    );
                }
            }
        }

        None
    }

    fn format_type_param_strict_null_predicate(
        &self,
        type_param: tsz_solver::types::TypeId,
        outer_type_params: Option<&NodeList>,
    ) -> String {
        let type_param_text = outer_type_params
            .filter(|tp| !tp.nodes.is_empty())
            .map(|tp| self.print_type_id_with_outer_type_params(type_param, tp))
            .unwrap_or_else(|| self.print_type_id(type_param));
        format!("{type_param_text} & ({{}} | undefined)")
    }
}
