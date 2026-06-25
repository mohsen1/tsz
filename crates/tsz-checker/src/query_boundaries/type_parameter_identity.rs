use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) fn contains_type_parameter_identity_shallow(
    db: &dyn TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    root: TypeId,
    target: TypeId,
) -> bool {
    tsz_solver::visitor::contains_type_parameter_identity_shallow(db, def_store, root, target)
}

pub(crate) fn constraint_references_type_param_identity_in_resolution_path(
    db: &dyn TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    root: TypeId,
    target: TypeId,
) -> bool {
    tsz_solver::visitor::constraint_references_type_param_identity_in_resolution_path(
        db, def_store, root, target,
    )
}

/// Whether `type_id` contains a generic `Application` reachable along the
/// base-constraint resolution path (union/intersection members, mapped key
/// source, index access). Used to gate alias expansion in TS2313 circular
/// constraint detection without touching the contextual-inference predicate.
pub(crate) fn contains_application_in_constraint_resolution_path(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::contains_application_in_constraint_resolution_path(db, type_id)
}

/// True when `type_id` (distributing over union/intersection members) is an
/// instantiable type whose base constraint is a union or includes
/// `null`/`undefined`. Mirrors tsc's `isGenericTypeWithUnionConstraint`; used to
/// decide whether a constraint-position reference is substituted with its base
/// constraint.
pub(crate) fn is_generic_type_with_union_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_generic_type_with_union_constraint(db, type_id)
}

/// True when `type_id` (distributing over union/intersection members) is an
/// instantiable type whose base constraint does not include `null`/`undefined`.
/// Mirrors tsc's `isGenericTypeWithoutNullableConstraint`; used for the
/// `obj[key]` deferred-`T[K]` exception to constraint-position substitution.
pub(crate) fn is_generic_type_without_nullable_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_generic_type_without_nullable_constraint(db, type_id)
}

/// Substitute a constraint-position reference's type with its base constraint,
/// distributing over union members (tsc's `mapType(type, getBaseConstraintOrType)`).
pub(crate) fn substitute_reference_base_constraints(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::type_queries::substitute_reference_base_constraints(db, type_id)
}

/// Like [`super::common::resolve_unbound_type_params_to_defaults`] but
/// additionally preserves any free type parameter whose declared name is in
/// `preserve_names`. Used at the `this`-member property-access boundary to keep
/// the enclosing class's own type parameters abstract even inside a nested
/// closure where the `TypeId`-keyed scope no longer carries them.
/// See [`tsz_solver::computation::resolve_unbound_type_params_to_defaults_preserving_names`].
pub(crate) fn resolve_unbound_type_params_to_defaults_preserving_names<S, N>(
    db: &dyn TypeDatabase,
    member_type: TypeId,
    in_scope: &std::collections::HashSet<TypeId, S>,
    preserve_names: &std::collections::HashSet<tsz_common::Atom, N>,
) -> TypeId
where
    S: std::hash::BuildHasher,
    N: std::hash::BuildHasher,
{
    tsz_solver::computation::resolve_unbound_type_params_to_defaults_preserving_names(
        db,
        member_type,
        in_scope,
        preserve_names,
    )
}
