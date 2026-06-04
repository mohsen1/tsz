include!("constraint_validation_large_methods/validate_type_args_against_params_5_2.rs");

use crate::query_boundaries::checkers::generic as query;
use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns true when an arity diagnostic was emitted inside `type_arg_idx`.
    fn type_arg_subtree_has_arity_error(&self, type_arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_arg_idx) else {
            return false;
        };
        let (start, end) = (node.pos, node.end);
        if end <= start {
            return false;
        }
        self.ctx
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, 2314 | 2315 | 2707) && d.start >= start && d.start < end)
    }

    fn type_arg_subtree_has_value_used_as_type_error(&self, type_arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_arg_idx) else {
            return false;
        };
        let (start, end) = (node.pos, node.end);
        if end <= start {
            return false;
        }
        let code = crate::diagnostics::diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF;
        self.ctx
            .diagnostics
            .iter()
            .any(|d| d.code == code && d.start >= start && d.start < end)
    }

    __tsz_split_constraint_validation_validate_type_args_against_params_5_2!();

    pub(super) fn ast_indexed_access_property_union_from_declaration(
        &mut self,
        type_arg: TypeId,
        arg_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(arg_idx)?;
        let indexed = self.ctx.arena.get_indexed_access_type(node)?;

        let db = self.ctx.types.as_type_database();
        let (object_type, _index_type) = query::index_access_components(db, type_arg)?;
        if matches!(object_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let object_type_for_check = self.evaluate_type_for_assignability(object_type);
        let object_type_for_check = self.resolve_lazy_type(object_type_for_check);
        let index_constraint = self
            .resolve_index_constraint_from_declaration(indexed.index_type, indexed.object_type)?;

        if !self.is_keyof_for_current_object(index_constraint, object_type, object_type_for_check) {
            return None;
        }

        let key_space = if let Some(keyof_operand) = query::keyof_operand(db, index_constraint) {
            self.get_keyof_type(keyof_operand)
        } else {
            self.evaluate_type_for_assignability(index_constraint)
        };
        let key_space = self.resolve_lazy_type(key_space);
        let value_type =
            self.constraint_check_indexed_access_value_type(object_type_for_check, key_space)?;
        let value_type = self.evaluate_type_for_assignability(value_type);
        let value_type = self.resolve_lazy_type(value_type);
        (!query::contains_free_type_parameters(self.ctx.types, value_type)).then_some(value_type)
    }
}
