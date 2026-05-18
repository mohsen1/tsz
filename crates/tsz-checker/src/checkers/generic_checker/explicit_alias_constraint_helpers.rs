use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{TypeId, TypeParamInfo};

impl<'a> CheckerState<'a> {
    pub(super) fn explicit_alias_type_parameter_constraint_satisfies_arg_constraint(
        &mut self,
        arg_idx: NodeIndex,
        type_arg: TypeId,
        constraint: TypeId,
        type_params: &[TypeParamInfo],
        type_args: &[TypeId],
    ) -> bool {
        let Some(constraint_node) = self.type_arg_explicit_constraint_node_in_ast(arg_idx) else {
            return false;
        };
        let explicit_base = self.get_type_from_type_node(constraint_node);
        if explicit_base == TypeId::UNKNOWN || explicit_base == type_arg {
            return false;
        }

        let constraint_resolved = self.resolve_lazy_type(constraint);
        let inst_constraint =
            self.instantiate_constraint_with_type_args(constraint_resolved, type_params, type_args);
        if inst_constraint == TypeId::UNKNOWN || inst_constraint == TypeId::ANY {
            return true;
        }

        let explicit_base_resolved = self.resolve_lazy_members_in_union(explicit_base);
        let explicit_base_for_check = self.evaluate_type_for_assignability(explicit_base_resolved);
        let inst_constraint_for_check = self.evaluate_type_for_assignability(inst_constraint);
        self.is_assignable_to(explicit_base_for_check, inst_constraint_for_check)
            || self.base_union_members_satisfy_constraint(
                explicit_base_for_check,
                inst_constraint_for_check,
            )
            || self
                .satisfies_array_like_constraint(explicit_base_for_check, inst_constraint_for_check)
    }
}
