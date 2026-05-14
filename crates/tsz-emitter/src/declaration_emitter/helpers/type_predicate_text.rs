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
                        .unwrap_or_else(|| self.print_type_id(type_id))
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
        let members = tsz_solver::type_queries::get_intersection_members(interner, type_id)?;
        let [left, right] = members.as_slice() else {
            return None;
        };

        let is_type_param_like = |id| tsz_solver::visitor::type_param_info(interner, id).is_some();
        let (type_param, union_type) = if is_type_param_like(*left) {
            (*left, *right)
        } else if is_type_param_like(*right) {
            (*right, *left)
        } else {
            return None;
        };

        let union_members = tsz_solver::type_queries::get_union_members(interner, union_type)?;
        let has_undefined = union_members.contains(&tsz_solver::types::TypeId::UNDEFINED);
        let has_empty_object = union_members
            .iter()
            .copied()
            .any(|member| tsz_solver::type_queries::is_empty_object_type(interner, member));
        if !has_undefined || !has_empty_object {
            return None;
        }

        let type_param_text = outer_type_params
            .filter(|type_params| !type_params.nodes.is_empty())
            .map(|type_params| self.print_type_id_with_outer_type_params(type_param, type_params))
            .unwrap_or_else(|| self.print_type_id(type_param));
        Some(format!("{type_param_text} & ({{}} | undefined)"))
    }
}
