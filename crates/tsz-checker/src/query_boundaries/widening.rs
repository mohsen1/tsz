//! Widening helpers exposed at the query boundary.
//!
//! Wraps solver widening primitives so checker callers don't reach into
//! `tsz_solver::*` directly (architecture rule: no inline solver function
//! calls in checker modules).

use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

/// Widen a type for inference resolution: deep-widens fresh literals while
/// preserving function/callable parameter and return types unchanged.
///
/// Mirrors tsc's `getInferredType` behavior — use this in JSX prop / call
/// argument inference paths where widening contravariant function param
/// types would produce types incompatible with the original argument under
/// strict-function-types.
pub(crate) fn widen_type_for_inference(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::operations::widening::widen_type_for_inference(db, type_id)
}

/// Widen a type for diagnostic display while preserving literal property types
/// of non-fresh objects. Fresh object literals still widen.
pub(crate) fn widen_type_for_display_preserving_non_fresh(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::operations::widening::widen_type_for_display_preserving_non_fresh(db, type_id)
}

/// Apply a `const` assertion to a type, recursively converting mutable literals
/// to their `readonly` / literal-preserving forms.
pub(crate) fn apply_const_assertion(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::operations::widening::apply_const_assertion(db, type_id)
}

/// Widen a fresh `let`/`var` initializer type, recursing into union members.
///
/// Like a plain `widen_type` but also widens fresh object/array constituents
/// nested inside a top-level union (e.g. `(1 | 2 | 3)[] | (4 | 5)[]` →
/// `number[]`), matching tsc's `getWidenedType`. Object freshness is respected,
/// so non-fresh alias unions are left untouched.
pub(crate) fn widen_type_for_mutable_binding(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::operations::widening::widen_type_for_mutable_binding(db, type_id)
}

/// Whether `type_id` is a *plain* object/array shape: `Object`,
/// `ObjectWithIndex`, `Array`, or `Tuple` only. Excludes `Function`,
/// `Callable`, `Mapped`, `Intersection`, `TypeParameter`, and `Lazy`.
///
/// Useful when opting in to deep object-literal widening without touching
/// function-shaped types or types that need to be resolved before their
/// kind is meaningful.
pub(crate) fn is_plain_object_or_array_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_type(db, type_id)
        || tsz_solver::type_queries::is_array_or_tuple_type(db, type_id)
}
