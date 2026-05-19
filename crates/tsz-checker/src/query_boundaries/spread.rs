//! Solver-boundary helpers used by the object-spread collector.
//!
//! Thin wrappers that keep checker code from inspecting solver internals
//! directly; the architecture contract requires solver-internal types to
//! be reached only through `query_boundaries/`.

use tsz_common::interner::Atom;
use tsz_solver::{DefId, TypeDatabase, TypeId};

pub(crate) fn unresolved_type_name_atom(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    tsz_solver::visitor::unresolved_type_name_atom(db, type_id)
}

pub(crate) fn make_application(db: &dyn TypeDatabase, base: TypeId, args: Vec<TypeId>) -> TypeId {
    db.application(base, args)
}

pub(crate) fn make_lazy(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn make_intersection(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn contains_unresolved_application(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::contains_unresolved_application(types, type_id)
}
