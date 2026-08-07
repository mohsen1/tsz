//! Query boundary for the "which property conflicts" question behind tsc's
//! TS18031 elaboration on a disjoint-object-literal intersection reduced to
//! `never`.

use crate::construction::TypeDatabase;
use crate::types::TypeId;
use tsz_common::interner::Atom;

/// Returns the first property name (in first-declared order across
/// `members`) whose occurrences across an intersection's members are
/// mutually unsatisfiable, forcing the whole intersection to `never`.
///
/// `members` must be independently re-evaluated from the source annotation
/// text — once `crate::intern::normalize`'s disjoint-object-literal check
/// collapses an intersection to `TypeId::NEVER` at intern time, the member
/// list is gone from the interned type itself, so callers need to walk the
/// declared annotation node and re-evaluate each member's type on their own
/// (see `declared_intersection_display.rs` in `tsz-checker` for that walk).
pub fn find_disjoint_object_literal_conflict_property(
    db: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Atom> {
    crate::intern::find_disjoint_object_literal_conflict_property(db, members)
}
