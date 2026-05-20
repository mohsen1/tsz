//! Array-like helpers for TS2344 constraint validation.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

/// `true` when `type_id` is structurally a `readonly` array/tuple shape —
/// either a `ReadonlyType(_)` wrapper or any union/intersection/type-parameter
/// whose array-like classification is `Readonly`. Returns `false` for plain
/// `Array`/`Tuple`, primitives, and non-array-like types.
fn source_is_readonly_array_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match query::classify_array_like(db, type_id) {
        query::ArrayLikeKind::Readonly(_) => true,
        query::ArrayLikeKind::Union(members) => members
            .iter()
            .all(|&m| source_is_readonly_array_like(db, m)),
        query::ArrayLikeKind::Intersection(members) => members
            .iter()
            .any(|&m| source_is_readonly_array_like(db, m)),
        _ => false,
    }
}

/// `true` when `type_id` is structurally a mutable array/tuple shape —
/// `Array`, `Tuple`, or a recursive container whose array-like classification
/// resolves to one of those. Used together with `source_is_readonly_array_like`
/// to gate the TS2344 readonly→mutable rejection.
fn target_is_mutable_array_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match query::classify_array_like(db, type_id) {
        query::ArrayLikeKind::Array(_) | query::ArrayLikeKind::Tuple => true,
        query::ArrayLikeKind::Union(members) => {
            members.iter().any(|&m| target_is_mutable_array_like(db, m))
        }
        query::ArrayLikeKind::Intersection(members) => {
            members.iter().all(|&m| target_is_mutable_array_like(db, m))
        }
        _ => false,
    }
}

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
        // A `readonly T[]` source must not satisfy a mutable `T[]` constraint:
        // tsc rejects that with TS4104, and the broader TS2344 path treats the
        // constraint as unsatisfied (e.g. `V extends readonly unknown[]` cannot
        // be used to instantiate `T extends unknown[]`). Before the element-
        // access readonly fix this was masked because indexing a readonly
        // array returned `ERROR`, making the fallback early-out below; with
        // indexing fixed, the readonly→mutable variance has to be enforced
        // here directly.
        if source_is_readonly_array_like(self.ctx.types, source)
            && target_is_mutable_array_like(self.ctx.types, target)
        {
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
