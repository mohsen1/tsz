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

/// Non-strict companion: map `null`/`undefined` to `any` in inferred
/// positions after ordinary widening (tsc's nullWideningType→anyType under
/// `strictNullChecks: false`).
pub(crate) fn widen_nullish_to_any_deep(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::operations::widening::widen_nullish_to_any_deep(db, type_id)
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

/// Widen a `const` declaration's fresh initializer: preserve a top-level
/// primitive literal (or union of them) while widening the mutable members of
/// fresh arrays/tuples/objects. `const c = cond ? ["x"] : []` → `string[]`,
/// `const c = cond ? "x" : "y"` → `"x" | "y"`.
pub(crate) fn widen_const_initializer(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::operations::widening::widen_const_initializer(db, type_id)
}

/// Widen bare `unique symbol` aliases in a fresh object/array literal's mutable
/// element positions to `symbol`, matching tsc's `getWidenedUniqueESSymbolType`
/// at a const/let binding: `const o = { m: cs }` → `{ m: symbol }`,
/// `const a = [cs]` → `symbol[]`. Preserves `readonly` (`as const`) positions
/// and leaves every non-unique-symbol type unchanged.
pub(crate) fn widen_unique_symbol_literal_elements(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::operations::widening::widen_unique_symbol_literal_elements(db, type_id)
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

// ── Redeclaration widening helpers ──

/// Widen a literal return type in a function-shaped type for TS2403 comparison.
///
/// For `Function` types (e.g., `(s: string) => 3`), widens the return type
/// from a literal to its base (e.g., `3` → `number`). Returns the original
/// type unchanged if it is not a `Function` or no widening is needed.
///
/// This is a thin boundary wrapper that keeps direct `type_queries` and
/// `widen_literal_type` calls out of checker modules.
pub(crate) fn widen_function_literal_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(shape) = tsz_solver::type_queries::get_function_shape(db, type_id) else {
        return type_id;
    };
    let widened_return =
        tsz_solver::operations::widening::widen_literal_type(db, shape.return_type);
    if widened_return != shape.return_type {
        tsz_solver::type_queries::replace_function_return_type(db, type_id, widened_return)
    } else {
        type_id
    }
}

/// Widen literal return types in callable call-signatures for TS2403 comparison.
///
/// For `Callable` types (e.g., `{ (s: string): 3 }`), widens each call
/// signature's return type from a literal to its base (e.g., `3` → `number`).
/// Returns the original type unchanged if it is not a `Callable` or no
/// widening is needed.
///
/// This is a thin boundary wrapper that encapsulates solver `TypeData::Callable`
/// inspection so checker modules never touch `.lookup()` or `TypeData` directly.
pub(crate) fn widen_callable_literal_return_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    let Some(callable) = tsz_solver::type_queries::get_callable_shape(db, type_id) else {
        return type_id;
    };

    let mut any_changed = false;
    let new_call_sigs: Vec<_> = callable
        .call_signatures
        .iter()
        .map(|sig| {
            let widened = tsz_solver::operations::widening::widen_literal_type(db, sig.return_type);
            if widened != sig.return_type {
                any_changed = true;
                let mut new_sig = sig.clone();
                new_sig.return_type = widened;
                new_sig
            } else {
                sig.clone()
            }
        })
        .collect();

    if any_changed {
        let mut new_shape = (*callable).clone();
        new_shape.call_signatures = new_call_sigs;
        db.callable(new_shape)
    } else {
        type_id
    }
}
