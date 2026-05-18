use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn base_union_members_satisfy_constraint(
        &mut self,
        base: TypeId,
        constraint: TypeId,
    ) -> bool {
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, base)
        else {
            return false;
        };
        let original_constraint = constraint;
        let constraint = self.resolve_lazy_type(constraint);
        let constraint = self.evaluate_type_for_assignability(constraint);
        !members.is_empty()
            && members.iter().all(|&member| {
                if self.member_extends_constraint_heritage(member, original_constraint) {
                    return true;
                }
                let member = self.resolve_lazy_type(member);
                let member = self.evaluate_type_for_assignability(member);
                self.is_assignable_to(member, constraint)
                    || self.satisfies_array_like_constraint(member, constraint)
            })
    }

    pub(crate) fn member_extends_constraint_heritage(
        &mut self,
        member: TypeId,
        constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();
        let member_sym = self
            .ctx
            .resolve_type_to_symbol_id(member)
            .or_else(|| {
                query::lazy_def_id(db, member)
                    .and_then(|def| self.ctx.def_to_symbol_id_with_fallback(def))
            })
            .or_else(|| self.symbol_id_for_heritage_type_name(member));
        let constraint_sym = self
            .ctx
            .resolve_type_to_symbol_id(constraint)
            .or_else(|| {
                query::lazy_def_id(db, constraint)
                    .and_then(|def| self.ctx.def_to_symbol_id_with_fallback(def))
            })
            .or_else(|| self.symbol_id_for_heritage_type_name(constraint));
        let (Some(member_sym), Some(constraint_sym)) = (member_sym, constraint_sym) else {
            return false;
        };
        self.interface_extends_symbol(member_sym, constraint_sym)
            && !self.member_has_conflicting_constraint_property(member, constraint)
    }
}
