//! Expression result construction boundary.
//!
//! Computation code owns AST classification, diagnostics, relation probes, and
//! collapse policy. This module owns the solver result surfaces those decisions
//! produce for expression operators.

use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) fn empty_object_type(db: &dyn TypeDatabase) -> TypeId {
    db.object(Vec::new())
}

pub(crate) fn nullish_coalescing_union(
    db: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> TypeId {
    db.union2(left, right)
}

pub(crate) fn conditional_branch_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn literal_index_access_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union_preserve_members(members)
}

pub(crate) fn typeof_result_union(db: &dyn TypeDatabase) -> TypeId {
    db.union(vec![
        db.literal_string("string"),
        db.literal_string("number"),
        db.literal_string("bigint"),
        db.literal_string("boolean"),
        db.literal_string("symbol"),
        db.literal_string("undefined"),
        db.literal_string("object"),
        db.literal_string("function"),
    ])
}
