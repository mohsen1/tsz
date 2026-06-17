//! Widening helpers exposed at the query boundary.
//!
//! Wraps solver widening primitives so checker callers don't reach into
//! `tsz_solver::*` directly (architecture rule: no inline solver function
//! calls in checker modules).

use tsz_solver::construction::TypeDatabase;
use tsz_solver::{ObjectShape, PropertyInfo, TypeId};

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

/// The [`ObjectShape`] of `type_id` when it is an object type, else `None`.
///
/// Exposed alongside the const-assertion widening helpers so the return-type
/// inference path can read an object literal's shape to widen a subset of its
/// properties (preserving const-asserted leaves) through this narrow boundary.
pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

/// Rebuild an object type from `original` with `new_props`, preserving index
/// signatures, flags (including `FRESH_LITERAL`), declaring symbol, and display
/// provenance. Returns `original` unchanged when the properties are unchanged.
///
/// Pairs with [`object_shape_for_type`] for AST-driven partial widening that
/// must keep the object's freshness/identity metadata.
pub(crate) fn rebuild_object_with_shape_metadata(
    db: &dyn TypeDatabase,
    original: TypeId,
    shape: &ObjectShape,
    new_props: Vec<PropertyInfo>,
) -> TypeId {
    tsz_solver::operations::widening::rebuild_object_with_shape_metadata(
        db, original, shape, new_props,
    )
}
