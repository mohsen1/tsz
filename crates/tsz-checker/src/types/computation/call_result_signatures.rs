//! Signature-shape helpers for call result diagnostics.

use crate::query_boundaries::common;
use crate::state::CheckerState;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_solver::TypeId;

fn type_has_construct_signature_deep(db: &dyn tsz_solver::TypeDatabase, ty: TypeId) -> bool {
    common::has_construct_signatures(db, ty)
        || common::is_constructor_like_type(db, ty)
        || common::union_members(db, ty).is_some_and(|members| {
            members
                .iter()
                .copied()
                .any(|member| type_has_construct_signature_deep(db, member))
        })
}

fn type_has_call_or_construct_signature_deep(
    db: &dyn tsz_solver::TypeDatabase,
    ty: TypeId,
) -> bool {
    type_has_construct_signature_deep(db, ty)
        || common::call_signatures_for_type(db, ty).is_some_and(|signatures| !signatures.is_empty())
        || common::union_members(db, ty).is_some_and(|members| {
            members
                .iter()
                .copied()
                .any(|member| type_has_call_or_construct_signature_deep(db, member))
        })
}

impl<'a> CheckerState<'a> {
    pub(super) fn is_generic_indexed_access_surface(&self, type_id: TypeId) -> bool {
        self.generic_indexed_access_surface_inner(type_id)
            || self
                .ctx
                .types
                .get_display_alias(type_id)
                .is_some_and(|alias| self.generic_indexed_access_surface_inner(alias))
    }

    fn generic_indexed_access_surface_inner(&self, type_id: TypeId) -> bool {
        common::contains_generic_indexed_access_surface(self.ctx.types, type_id)
    }

    pub(super) fn callable_mismatch_cascades_from_constraint_diagnostic(
        &self,
        actual: TypeId,
        expected: TypeId,
    ) -> bool {
        self.ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT)
            && type_has_construct_signature_deep(self.ctx.types, actual)
            && type_has_call_or_construct_signature_deep(self.ctx.types, expected)
    }
}
