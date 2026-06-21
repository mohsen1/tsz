use crate::caches::db::TypeDatabase;
use crate::types::{InferencePriority, TypeData, TypeId};

use super::infer::{InferenceContext, InferenceVar};

impl InferenceContext<'_> {
    pub(super) fn add_source_type_param_candidate(
        &mut self,
        var: InferenceVar,
        target: TypeId,
        priority: InferencePriority,
    ) {
        if self.type_is_own_original_type_param(var, target) {
            return;
        }

        if self.in_contra_mode {
            self.add_contra_candidate(var, target, priority);
        } else {
            self.add_upper_bound(var, target);
        }
    }
}

pub(super) fn constraint_is_nullable_union(db: &dyn TypeDatabase, constraint: TypeId) -> bool {
    let Some(TypeData::Union(members)) = db.lookup(constraint) else {
        return false;
    };
    db.type_list(members)
        .iter()
        .any(|&member| member.is_nullable())
}
