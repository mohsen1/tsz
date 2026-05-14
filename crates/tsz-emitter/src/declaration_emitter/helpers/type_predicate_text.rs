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
        let tsz_solver::types::TypeData::Intersection(list_id) = interner.lookup(type_id)? else {
            return None;
        };
        let members = interner.type_list(list_id);
        let [left, right] = members.as_ref() else {
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

        let tsz_solver::types::TypeData::Union(union_id) = interner.lookup(union_type)? else {
            return None;
        };
        let union_members = interner.type_list(union_id);
        let has_undefined = union_members.contains(&tsz_solver::types::TypeId::UNDEFINED);
        let has_empty_object = union_members
            .iter()
            .copied()
            .any(|member| self.type_id_is_empty_object(member));
        if !has_undefined || !has_empty_object {
            return None;
        }

        let type_param_text = outer_type_params
            .filter(|type_params| !type_params.nodes.is_empty())
            .map(|type_params| self.print_type_id_with_outer_type_params(type_param, type_params))
            .unwrap_or_else(|| self.print_type_id(type_param));
        Some(format!("{type_param_text} & ({{}} | undefined)"))
    }

    fn type_id_is_empty_object(&self, type_id: tsz_solver::types::TypeId) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };
        if type_id.is_intrinsic() {
            return false;
        }
        tsz_solver::visitor::object_shape_id(interner, type_id)
            .map(|shape_id| interner.object_shape(shape_id))
            .is_some_and(|shape| {
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
                    && shape.symbol.is_none()
            })
    }
}
