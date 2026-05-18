use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn merged_interface_sibling_constraint_satisfies_type_arg_constraint(
        &mut self,
        arg_idx: NodeIndex,
        required_constraint: TypeId,
    ) -> bool {
        let Some(type_param_name) = self.type_arg_identifier_name(arg_idx) else {
            return false;
        };
        let Some((interface_idx, interface_name, interface_parent)) =
            self.enclosing_interface_for_type_arg(arg_idx)
        else {
            return false;
        };

        let mut sibling_constraints = Vec::new();
        for (idx, node) in self.ctx.arena.nodes.iter().enumerate() {
            let sibling_idx = NodeIndex(idx as u32);
            if sibling_idx == interface_idx || node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                continue;
            }
            let sibling_parent = self
                .ctx
                .arena
                .get_extended(sibling_idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
            if sibling_parent != interface_parent {
                continue;
            }
            let Some(iface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            let Some(sibling_name) = self.ctx.arena.get_identifier_text(iface.name) else {
                continue;
            };
            if sibling_name != interface_name {
                continue;
            }
            let Some(type_parameters) = &iface.type_parameters else {
                continue;
            };
            for &param_idx in &type_parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                    continue;
                };
                let Some(param_name) = self.ctx.arena.get_identifier_text(param.name) else {
                    continue;
                };
                if param_name == type_param_name && param.constraint != NodeIndex::NONE {
                    sibling_constraints.push(param.constraint);
                }
            }
        }

        let required = self.resolve_lazy_type(required_constraint);
        sibling_constraints.into_iter().any(|constraint_idx| {
            let candidate = self.get_type_from_type_node(constraint_idx);
            if matches!(candidate, TypeId::ERROR | TypeId::UNKNOWN) {
                return false;
            }
            let candidate = self.resolve_lazy_type(candidate);
            let db = self.ctx.types.as_type_database();
            let required_is_callable =
                query::is_callable_type(db, required) || self.is_function_constraint(required);
            let candidate_is_callable =
                query::is_callable_type(db, candidate) || self.is_function_constraint(candidate);
            if required_is_callable && candidate_is_callable {
                return true;
            }
            let candidate_evaluated = self.evaluate_type_for_assignability(candidate);
            self.is_assignable_to(candidate, required)
                || self.is_assignable_to(candidate_evaluated, required)
                || self.satisfies_array_like_constraint(candidate_evaluated, required)
        })
    }

    fn enclosing_interface_for_type_arg(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<(NodeIndex, String, NodeIndex)> {
        let arg_node = self.ctx.arena.get(arg_idx)?;
        let mut best: Option<(u32, NodeIndex, String, NodeIndex)> = None;
        for (idx, node) in self.ctx.arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                && node.pos <= arg_node.pos
                && node.end >= arg_node.end
            {
                let interface_idx = NodeIndex(idx as u32);
                let iface = self.ctx.arena.get_interface(node)?;
                let name = self.ctx.arena.get_identifier_text(iface.name)?.to_string();
                let parent = self
                    .ctx
                    .arena
                    .get_extended(interface_idx)
                    .map_or(NodeIndex::NONE, |ext| ext.parent);
                let width = node.end.saturating_sub(node.pos);
                if best
                    .as_ref()
                    .is_none_or(|(best_width, _, _, _)| width < *best_width)
                {
                    best = Some((width, interface_idx, name, parent));
                }
            }
        }
        best.map(|(_, idx, name, parent)| (idx, name, parent))
    }
}
