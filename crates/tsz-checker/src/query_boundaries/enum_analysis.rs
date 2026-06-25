//! Enum and enum-adjacent checker queries.
//!
//! These wrappers keep enum utility code off the broad `common` quarantine
//! barrel while the underlying solver queries remain the semantic owner.

use std::sync::Arc;

use tsz_solver::construction::TypeDatabase;
use tsz_solver::{ObjectShape, TypeId};

pub(crate) fn enum_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    super::common::enum_def_id(db, type_id)
}

/// The structural member-value union of an enum type (e.g. `"red" | "blue"` for
/// a string enum, `0 | 1` for a numeric enum). Returns `None` when `type_id` is
/// not an enum type. This is the enum's comparison/overlap value-set.
pub(crate) fn enum_member_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    super::common::enum_member_type(db, type_id)
}

/// When exactly one of `(left, right)` is an enum (and the other is not),
/// returns the operand pair with the enum side replaced by its member-value
/// union — the form an overlap/comparability check should relate. Returns `None`
/// otherwise (neither is an enum, or both are).
///
/// `tsc` relates an enum to a non-enum literal/primitive/union through this
/// member union, so `Color === "red"` overlaps (a member value) while
/// `Color === "green"` does not. Enum-vs-enum comparisons stay nominal — two
/// different enums never overlap even with equal member values, and two members
/// of the same enum compare by their (distinct) values — so the both-enum case
/// is left to the nominal path.
pub(crate) fn enum_comparison_operands(
    db: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> Option<(TypeId, TypeId)> {
    match (
        enum_def_id(db, left).is_some(),
        enum_def_id(db, right).is_some(),
    ) {
        (true, false) => enum_member_type(db, left).map(|members| (members, right)),
        (false, true) => enum_member_type(db, right).map(|members| (left, members)),
        _ => None,
    }
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    super::common::type_parameter_constraint(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Arc<ObjectShape>> {
    super::common::object_shape_for_type(db, type_id)
}
