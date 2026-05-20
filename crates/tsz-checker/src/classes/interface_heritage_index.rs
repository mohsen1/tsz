use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_type_alias_base_properties_against_derived_string_index(
        &mut self,
        base_type: TypeId,
        string_index_value: TypeId,
        string_index_node: NodeIndex,
    ) {
        let resolved_base = self.resolve_type_query_type(base_type);
        let base_for_props = self.evaluate_type_for_assignability(resolved_base);
        let mut prop_names: Vec<String> =
            crate::query_boundaries::diagnostics::collect_property_name_atoms_for_diagnostics(
                self.ctx.types,
                base_for_props,
                5,
            )
            .into_iter()
            .map(|atom| self.ctx.types.resolve_atom_ref(atom).to_string())
            .collect();

        let base_shape_id =
            crate::query_boundaries::common::object_shape_id(self.ctx.types, base_for_props)
                .or_else(|| {
                    crate::query_boundaries::common::object_with_index_shape_id(
                        self.ctx.types,
                        base_for_props,
                    )
                });
        if let Some(base_shape_id) = base_shape_id {
            let base_shape = self.ctx.types.object_shape(base_shape_id);
            for prop in &base_shape.properties {
                prop_names.push(self.ctx.types.resolve_atom(prop.name));
            }
        }

        prop_names.sort();
        prop_names.dedup();
        for prop_name in prop_names {
            if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name) {
                continue;
            }
            let prop_result = self.resolve_property_access_with_env(base_for_props, &prop_name);
            let crate::query_boundaries::common::PropertyAccessResult::Success {
                type_id: prop_type,
                ..
            } = prop_result
            else {
                continue;
            };
            if self.type_contains_error(prop_type)
                || self.is_assignable_to(prop_type, string_index_value)
            {
                continue;
            }
            let prop_type_str = self.format_type(prop_type);
            let index_type_str = self.format_type(string_index_value);
            self.error_at_node_msg(
                string_index_node,
                diagnostic_codes::PROPERTY_OF_TYPE_IS_NOT_ASSIGNABLE_TO_INDEX_TYPE,
                &[&prop_name, &prop_type_str, "string", &index_type_str],
            );
        }
    }
}
