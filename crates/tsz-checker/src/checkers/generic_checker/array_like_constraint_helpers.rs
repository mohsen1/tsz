//! Array-like helpers for TS2344 constraint validation.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn satisfies_array_like_constraint(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);
        if !self.tuple_constraint_accepts_array_like_source(target) {
            return false;
        }
        let target_elem = crate::query_boundaries::checkers::call::array_element_type_for_type(
            self.ctx.types,
            target,
        )
        .unwrap_or_else(|| self.get_element_access_type(target, TypeId::NUMBER, Some(0)));
        if target_elem == TypeId::ERROR {
            return false;
        }

        if !self.is_array_like_type(source) && !self.has_structural_array_surface(source, target) {
            return false;
        }

        if target_elem == TypeId::ANY {
            return true;
        }

        let source_elem = self.get_element_access_type(source, TypeId::NUMBER, Some(0));
        source_elem != TypeId::ERROR
            && (self.is_assignable_to(source_elem, target_elem)
                || ((source_elem != source || target_elem != target)
                    && self.satisfies_array_like_constraint(source_elem, target_elem)))
    }

    fn tuple_constraint_accepts_array_like_source(&self, target: TypeId) -> bool {
        let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, target)
        else {
            return true;
        };

        elements.len() == 1 && elements[0].rest
    }

    fn has_structural_array_surface(&self, source: TypeId, target: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();

        let Some(shape) = query::get_object_shape(db, source) else {
            return false;
        };
        if shape.number_index.is_none() {
            return false;
        }

        for name in [
            "length",
            "concat",
            "slice",
            "join",
            "indexOf",
            "lastIndexOf",
            "every",
        ] {
            if !query::has_property_by_name(db, source, name) {
                return false;
            }
        }

        if !matches!(
            query::classify_array_like(db, target),
            query::ArrayLikeKind::Readonly(_)
        ) && !query::has_property_by_name(db, source, "push")
        {
            return false;
        }

        true
    }
}
