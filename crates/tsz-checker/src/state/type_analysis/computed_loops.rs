use crate::query_boundaries::common::{is_keyof_type, is_type_parameter_like};
use crate::query_boundaries::flow as flow_boundary;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn compute_for_in_of_variable_type(
        &mut self,
        decl_idx: NodeIndex,
    ) -> Option<TypeId> {
        let decl_parent = self
            .ctx
            .arena
            .get_extended(decl_idx)
            .map(|ext| ext.parent)?;
        let decl_list_node = self.ctx.arena.get(decl_parent)?;
        if decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return None;
        }

        let list_parent = self
            .ctx
            .arena
            .get_extended(decl_parent)
            .map(|ext| ext.parent)?;
        let for_node = self.ctx.arena.get(list_parent)?;

        if for_node.kind == syntax_kind_ext::FOR_OF_STATEMENT {
            let for_data = self.ctx.arena.get_for_in_of(for_node).cloned()?;
            let expr_type = self.get_type_of_node(for_data.expression);
            Some(self.for_of_element_type(expr_type, for_data.await_modifier))
        } else if for_node.kind == syntax_kind_ext::FOR_IN_STATEMENT {
            let for_data = self.ctx.arena.get_for_in_of(for_node).cloned()?;
            let expr_type = self.get_type_of_node(for_data.expression);
            Some(self.compute_for_in_variable_type(expr_type))
        } else {
            None
        }
    }

    pub(super) fn try_resolve_named_export_via_export_equals_type(
        &mut self,
        module_name: &str,
        export_name: &str,
    ) -> Option<tsz_solver::TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;

        let exports_table = self.resolve_effective_module_exports(module_name)?;
        let export_equals_sym = exports_table.get("export=")?;

        let export_type = self.get_type_of_symbol(export_equals_sym);
        if export_type == tsz_solver::TypeId::ERROR || export_type == tsz_solver::TypeId::ANY {
            return None;
        }
        if export_name == "default" {
            return Some(export_type);
        }

        match self.resolve_property_access_with_env(export_type, export_name) {
            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
            _ => None,
        }
    }

    pub(crate) fn compute_for_in_variable_type(&mut self, expr_type: TypeId) -> TypeId {
        // Route nullish removal through the flow observation boundary.
        let non_nullable = flow_boundary::remove_nullish_for_iteration(self.ctx.types, expr_type);

        // For concrete (non-generic) types, always return `string`.
        // Same fix as FlowAnalyzer::for_in_variable_type: computing keyof
        // on concrete types creates unevaluated KeyOf nodes that leak into
        // the variable type as `keyof T & string`.
        if !self.contains_type_parameters_cached(non_nullable) {
            return TypeId::STRING;
        }

        let keyof_type = self.ctx.types.factory().keyof(non_nullable);
        let keyof_evaluated = self.evaluate_type_with_env(keyof_type);

        if is_type_parameter_like(self.ctx.types, keyof_evaluated)
            || is_keyof_type(self.ctx.types, keyof_evaluated)
        {
            self.ctx
                .types
                .factory()
                .intersection2(keyof_evaluated, TypeId::STRING)
        } else {
            self.ctx
                .types
                .factory()
                .intersection2(keyof_type, TypeId::STRING)
        }
    }
}
