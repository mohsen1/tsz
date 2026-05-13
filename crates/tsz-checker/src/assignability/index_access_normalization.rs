use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn normalize_index_access_for_assignability(
        &mut self,
        ty: TypeId,
        depth: u8,
    ) -> TypeId {
        if depth > 8 {
            return ty;
        }

        let reduced = self.reduce_literal_index_access_property_types(ty);
        if reduced != ty {
            return self.normalize_index_access_for_assignability(reduced, depth + 1);
        }

        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty) {
            let evaluated = self.evaluate_type_for_assignability(ty);
            if evaluated != ty && evaluated != TypeId::ERROR {
                return self.normalize_index_access_for_assignability(evaluated, depth + 1);
            }
        }

        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, ty) {
            let mut changed = false;
            let normalized_members: Vec<_> = members
                .into_iter()
                .map(|member| {
                    let normalized =
                        self.normalize_index_access_for_assignability(member, depth + 1);
                    changed |= normalized != member;
                    normalized
                })
                .collect();
            if changed {
                return self.ctx.types.factory().union(normalized_members);
            }
        }

        ty
    }
}
