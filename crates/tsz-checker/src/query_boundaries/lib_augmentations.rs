use tsz_solver::construction::TypeDatabase;
use tsz_solver::{DefId, ObjectFlags, TypeId};

/// True when `candidate`'s named-property set is a strict subset of
/// `current`'s named-property set.
///
/// This is the semantic boundary for lib-interface finalized-body publication:
/// checker orchestration decides when a lib body may be published, while this
/// helper owns the object-shape/member-set question used to reject
/// heritage-thin re-publications.
pub(crate) fn lib_body_strictly_loses_members(
    db: &dyn TypeDatabase,
    current: TypeId,
    candidate: TypeId,
) -> bool {
    if current == candidate {
        return false;
    }
    let Some(current_shape) = super::common::object_shape_for_type(db, current) else {
        return false;
    };
    let Some(candidate_shape) = super::common::object_shape_for_type(db, candidate) else {
        return false;
    };
    if candidate_shape.properties.len() >= current_shape.properties.len() {
        return false;
    }
    // Every candidate member must already be present in `current` (no
    // additions) and `candidate` is strictly smaller (checked above), so it is
    // missing at least one member `current` has. Lib interface bodies carry few
    // members, so a linear scan beats allocating a name set.
    candidate_shape.properties.iter().all(|cand| {
        current_shape
            .properties
            .iter()
            .any(|cur| cur.name == cand.name)
    })
}

pub(crate) fn suppresses_module_augmentation_lookup(
    db: &dyn TypeDatabase,
    object_type: TypeId,
) -> bool {
    super::common::object_shape_for_type(db, object_type).is_some_and(|shape| {
        shape
            .flags
            .contains(ObjectFlags::NO_MODULE_AUGMENTATION_LOOKUP)
    })
}

pub(crate) fn is_lazy_def_identity(db: &dyn TypeDatabase, ty: TypeId, def_id: DefId) -> bool {
    super::definition_identity::is_lazy_def_identity(db, ty, def_id)
}

pub(crate) fn type_id_is_known_to_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    super::common::type_id_is_known_to_db(db, type_id)
}

pub(crate) fn has_construct_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    super::checkers::constructor::has_construct_signatures(db, type_id)
}

#[cfg(test)]
mod monotone_publication_tests {
    //! Unit guard for the membership-monotone lib-interface body publication
    //! that fixes the immer `SetIterator`/`MapIterator` `next`-missing false
    //! positives (#13942): a heritage-thin re-derivation of a lib interface body
    //! must never clobber a more-complete one in the shared store / per-file
    //! `type_env`.
    use super::lib_body_strictly_loses_members;
    use tsz_solver::construction::{QueryDatabase, TypeInterner};
    use tsz_solver::{PropertyInfo, TypeId};

    fn obj(types: &TypeInterner, names: &[&str]) -> TypeId {
        let props = names
            .iter()
            .map(|n| PropertyInfo::new(types.intern_string(n), TypeId::NUMBER))
            .collect();
        types.factory().object(props)
    }

    #[test]
    fn thin_body_dropping_inherited_member_is_rejected() {
        let types = TypeInterner::new();
        // `SetIterator` heritage-complete: own `[Symbol.iterator]` + inherited
        // `next` (from `IteratorObject` -> `Iterator`).
        let complete = obj(&types, &["__@iterator", "next"]);
        // Heritage-thin re-derivation: dropped the inherited `next`.
        let thin = obj(&types, &["__@iterator"]);
        assert!(
            lib_body_strictly_loses_members(&types, complete, thin),
            "a thin body that drops an inherited member must be rejected",
        );
        // Completion in the other order (thin published first, complete arriving)
        // is a superset and must still win.
        assert!(
            !lib_body_strictly_loses_members(&types, thin, complete),
            "heritage completion (growing the member set) must be allowed",
        );
    }

    #[test]
    fn growth_via_augmentation_is_allowed() {
        let types = TypeInterner::new();
        let base = obj(&types, &["a", "b"]);
        let augmented = obj(&types, &["a", "b", "c"]);
        assert!(!lib_body_strictly_loses_members(&types, base, augmented));
    }

    #[test]
    fn equal_member_set_is_not_a_loss() {
        let types = TypeInterner::new();
        let a = obj(&types, &["a", "b"]);
        let b = obj(&types, &["a", "b"]);
        // Structurally identical objects intern to the same `TypeId`, so this
        // also exercises the `current == candidate` short-circuit.
        assert!(!lib_body_strictly_loses_members(&types, a, b));
    }

    #[test]
    fn added_and_dropped_member_same_size_is_not_a_loss() {
        let types = TypeInterner::new();
        let current = obj(&types, &["a", "b"]);
        // Same size, but adds `c` and drops `b`: not a strict subset, so it is
        // not a pure membership loss and replacement proceeds.
        let candidate = obj(&types, &["a", "c"]);
        assert!(!lib_body_strictly_loses_members(&types, current, candidate));
    }

    #[test]
    fn non_object_bodies_allow_replacement() {
        let types = TypeInterner::new();
        let o = obj(&types, &["a"]);
        assert!(!lib_body_strictly_loses_members(&types, TypeId::NUMBER, o));
        assert!(!lib_body_strictly_loses_members(&types, o, TypeId::STRING));
    }
}
