//! Widening helpers exposed at the query boundary.
//!
//! Wraps solver widening primitives so checker callers don't reach into
//! `tsz_solver::*` directly (architecture rule: no inline solver function
//! calls in checker modules).

use tsz_solver::{TypeDatabase, TypeId};

/// Widen a type for inference resolution: deep-widens fresh literals while
/// preserving function/callable parameter and return types unchanged.
///
/// Mirrors tsc's `getInferredType` behavior — use this in JSX prop / call
/// argument inference paths where widening contravariant function param
/// types would produce types incompatible with the original argument under
/// strict-function-types.
pub(crate) fn widen_type_for_inference(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::widen_type_for_inference(db, type_id)
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
