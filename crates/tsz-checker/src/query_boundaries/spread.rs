//! Solver-boundary helpers used by the object-spread collector.
//!
//! Thin wrappers that keep checker code from inspecting solver internals
//! directly; the architecture contract requires solver-internal types to
//! be reached only through `query_boundaries/`.

use tsz_common::interner::Atom;
use tsz_solver::{DefId, TypeDatabase, TypeId};

/// If `type_id` is `TypeData::UnresolvedTypeName(atom)`, return the atom.
/// Used by checker code that needs to re-attempt qualified-name resolution
/// at evaluation time (e.g. for cross-file `Application` bases that the
/// lowering pass couldn't resolve when the alias body was first lowered).
pub(crate) fn unresolved_type_name_atom(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    tsz_solver::visitor::unresolved_type_name_atom(db, type_id)
}

/// Construct an `Application(base, args)` type.
pub(crate) fn make_application(db: &dyn TypeDatabase, base: TypeId, args: Vec<TypeId>) -> TypeId {
    db.application(base, args)
}

/// Construct a `Lazy(def_id)` type.
pub(crate) fn make_lazy(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

/// Construct an `Intersection(members)` type.
pub(crate) fn make_intersection(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

/// Returns true when `type_id` (recursively) contains an `Application`
/// whose base is `UnresolvedTypeName`. Used by the type-environment
/// evaluator to trigger a second pass with a wider resolver that can
/// recover the alias's `DefId` from the merged binder graph.
pub(crate) fn contains_unresolved_application(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::contains_unresolved_application(types, type_id)
}
