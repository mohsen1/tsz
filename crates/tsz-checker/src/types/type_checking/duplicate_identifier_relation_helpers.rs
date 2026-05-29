//! Relation helpers for duplicate declaration diagnostics.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn duplicate_decl_types_match(&mut self, left: TypeId, right: TypeId) -> bool {
        self.assign_relation_outcome(left, right).related
            && self.assign_relation_outcome(right, left).related
    }

    pub(super) fn duplicate_decl_type_matches_index(
        &mut self,
        property_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        self.assign_relation_outcome(property_type, index_type)
            .related
    }

    pub(super) fn duplicate_index_types_overlap(
        &mut self,
        local_type: TypeId,
        existing_type: TypeId,
    ) -> bool {
        self.assign_relation_outcome(local_type, existing_type)
            .related
            || self
                .assign_relation_outcome(existing_type, local_type)
                .related
    }
}
