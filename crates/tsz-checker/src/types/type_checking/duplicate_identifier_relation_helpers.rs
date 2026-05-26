//! Relation helpers for duplicate declaration diagnostics.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn duplicate_decl_types_match(&mut self, left: TypeId, right: TypeId) -> bool {
        self.diagnostic_relation_boolean_guard(left, right)
            && self.diagnostic_relation_boolean_guard(right, left)
    }

    pub(super) fn duplicate_decl_type_matches_index(
        &mut self,
        property_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        self.diagnostic_relation_boolean_guard(property_type, index_type)
    }

    pub(super) fn duplicate_index_types_overlap(
        &mut self,
        local_type: TypeId,
        existing_type: TypeId,
    ) -> bool {
        self.diagnostic_relation_boolean_guard(local_type, existing_type)
            || self.diagnostic_relation_boolean_guard(existing_type, local_type)
    }
}
