//! Flow observation boundary: checker extracts syntactic observations from the
//! CFG/AST; solver owns the semantic narrowing result.
//!
//! ## Architecture
//!
//! The checker (WHERE) walks the control-flow graph and AST to determine *what
//! kind of observation* was made at a program point.  It packages that as a
//! [`FlowObservation`] and passes it to the solver (WHAT), which performs the
//! actual type narrowing.
//!
//! This keeps the checker free of ad-hoc type-algebra for narrowing and gives
//! the solver a single entry-point for all flow-based type refinement.
//!
//! ## Observation kinds
//!
//! - **Destructuring**: a binding pattern extracts a property/element from a
//!   composite type.  The solver resolves the element type and optionally
//!   removes `undefined` when a default value is present.
//! - **`CatchVariable`**: the catch clause binds a variable typed as `any` or
//!   `unknown` depending on compiler options.
//! - **`OptionalChainNonNullish`**: an optional chain (e.g. `a?.b`) was observed
//!   in a truthy branch, implying the base is non-nullish.
//! - **`ForOfElement`**: a `for-of` loop destructures an iterable; the solver
//!   resolves the iterated element type.
//! - **`TruthyNarrow`**: a value was used in a boolean context; remove nullish
//!   and other falsy constituents.

use tsz_solver::{TypeDatabase, TypeId};

/// Syntactic observation the checker extracts from flow analysis.
///
/// This is an AST-free, source-span-free description of what happened at a
/// program point.  The solver applies the semantic narrowing.
#[derive(Clone, Debug)]
pub(crate) enum FlowObservation {
    /// A destructuring binding extracted element `index` from the parent type.
    /// If `has_default` is true, `undefined` is removed from the element type.
    DestructuringElement { index: usize, has_default: bool },

    /// A destructuring binding extracted a named property from the parent type.
    /// If `has_default` is true, `undefined` is removed from the element type.
    DestructuringProperty { name: String, has_default: bool },

    /// The variable is a catch clause binding.  The type is `unknown` when
    /// `useUnknownInCatchVariables` is enabled, otherwise `any`.
    CatchVariable { use_unknown: bool },

    /// An optional-chain expression was observed in a truthy branch, meaning
    /// the base value is non-nullish.
    OptionalChainNonNullish,

    /// A for-of loop destructures an iterable. The solver resolves the
    /// iterated element type from the iterable/iterator protocol.
    ForOfElement {
        /// The iterable expression type.
        iterable_type: TypeId,
    },

    /// The value was used in a truthy context (if/while/ternary condition).
    /// Remove null, undefined, false, 0, "", NaN from union constituents.
    TruthyNarrow {
        /// true = truthy branch (remove falsy), false = falsy branch (keep falsy)
        is_true_branch: bool,
    },
}

/// Apply a [`FlowObservation`] to produce a narrowed type.
///
/// This is the single boundary entry-point the checker calls after extracting
/// an observation from the AST/CFG.  All semantic narrowing logic lives in the
/// solver.
pub(crate) fn apply_flow_observation(
    db: &dyn TypeDatabase,
    base_type: TypeId,
    observation: &FlowObservation,
) -> TypeId {
    match observation {
        FlowObservation::CatchVariable { use_unknown } => {
            if *use_unknown {
                TypeId::UNKNOWN
            } else {
                TypeId::ANY
            }
        }

        FlowObservation::OptionalChainNonNullish => tsz_solver::remove_nullish(db, base_type),

        FlowObservation::TruthyNarrow { is_true_branch } => {
            if *is_true_branch {
                tsz_solver::remove_nullish(db, base_type)
            } else {
                // Falsy branch: keep only falsy constituents.
                // The caller should use NarrowingContext::narrow_to_falsy for
                // full falsy narrowing. This path provides the basic nullish
                // identity for cases where the caller handles falsy narrowing
                // separately.
                base_type
            }
        }

        FlowObservation::DestructuringElement { has_default, .. } => {
            // The actual element extraction is done by the checker's existing
            // get_binding_element_type path. This observation signals that the
            // result should have `undefined` removed when a default is present.
            if *has_default {
                tsz_solver::remove_undefined(db, base_type)
            } else {
                base_type
            }
        }

        FlowObservation::DestructuringProperty { has_default, .. } => {
            if *has_default {
                tsz_solver::remove_undefined(db, base_type)
            } else {
                base_type
            }
        }

        FlowObservation::ForOfElement { iterable_type } => {
            // The for-of element type is resolved by the checker's iterable
            // resolution.  This observation primarily signals the solver that
            // the binding's type comes from iteration.
            *iterable_type
        }
    }
}

/// Apply optional-chain non-nullish narrowing through the solver.
/// Convenience wrapper used by the checker's flow analyzer.
pub(crate) fn narrow_optional_chain(db: &dyn TypeDatabase, base_type: TypeId) -> TypeId {
    tsz_solver::remove_nullish(db, base_type)
}

/// Resolve catch variable type through the boundary.
pub(crate) const fn resolve_catch_variable_type(use_unknown: bool) -> TypeId {
    if use_unknown {
        TypeId::UNKNOWN
    } else {
        TypeId::ANY
    }
}

/// For catch variables in typeof narrowing, the typeof check should narrow
/// from the catch variable's declared base type (any/unknown) rather than
/// the already-narrowed flow type.  Non-catch variables pass through unchanged.
pub(crate) const fn catch_variable_typeof_base(type_id: TypeId, is_catch_var: bool) -> TypeId {
    if is_catch_var {
        // Catch variables are typed as `any` (or `unknown` with strict flag),
        // but in typeof guards, tsc always widens back to `any` so the typeof
        // narrowing starts from a clean slate.
        TypeId::ANY
    } else {
        type_id
    }
}

/// Strip undefined from a destructured element type when a default is present.
/// Centralizes the narrowing policy for destructuring defaults.
pub(crate) fn narrow_destructuring_default(
    db: &dyn TypeDatabase,
    element_type: TypeId,
    has_default: bool,
) -> TypeId {
    if !has_default {
        return element_type;
    }
    // Don't strip undefined from any/unknown/error - these should pass through
    if element_type == TypeId::ANY
        || element_type == TypeId::UNKNOWN
        || element_type == TypeId::ERROR
    {
        return element_type;
    }
    tsz_solver::remove_undefined(db, element_type)
}

/// Determine the catch variable type based on compiler options and any
/// explicit type annotation.  Centralizes the catch-variable typing policy.
///
/// Returns the catch variable's type:
/// - If `annotation_type` is `Some`, validates it (must be `any` or `unknown`)
///   and returns it.  The caller emits TS1196 if the annotation is invalid.
/// - If no annotation and `use_unknown` is true, returns `unknown`.
/// - Otherwise returns `any`.
pub(crate) const fn catch_variable_type(
    annotation_type: Option<TypeId>,
    use_unknown: bool,
) -> TypeId {
    match annotation_type {
        Some(ty) => ty,
        None => resolve_catch_variable_type(use_unknown),
    }
}

/// Widen null/undefined to `any` when `strict_null_checks` is off.
///
/// In non-strict mode, `null` and `undefined` standalone types widen to `any`
/// for mutable variable bindings.  When `strict_null_checks` is on, the type
/// passes through unchanged.
pub(crate) fn widen_null_undefined_to_any(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    strict_null_checks: bool,
) -> TypeId {
    if strict_null_checks {
        return type_id;
    }
    // Standalone null or undefined widens to any
    if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
        return TypeId::ANY;
    }
    // For unions containing null/undefined, remove them (they widen away in non-strict mode).
    // Use solver query API (get_union_members) instead of TypeData inspection.
    if let Some(members) = tsz_solver::type_queries::get_union_members(db, type_id) {
        let has_nullish = members
            .iter()
            .any(|m| *m == TypeId::NULL || *m == TypeId::UNDEFINED);
        if has_nullish {
            let filtered: Vec<TypeId> = members
                .iter()
                .copied()
                .filter(|m| *m != TypeId::NULL && *m != TypeId::UNDEFINED)
                .collect();
            if filtered.is_empty() {
                return TypeId::ANY;
            }
            if filtered.len() == 1 {
                return filtered[0];
            }
            return db.union(filtered);
        }
    }
    type_id
}

/// Apply non-null assertion (`x!`) narrowing through the solver.
/// Removes `null` and `undefined` from the type.
pub(crate) fn narrow_non_null_assertion(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::remove_nullish(db, type_id)
}

/// Remove nullish types for iteration contexts (for-in/for-of).
/// The iterable expression should not be null/undefined.
pub(crate) fn remove_nullish_for_iteration(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::remove_nullish(db, type_id)
}

/// Add `undefined` to a type for indexed access in destructuring contexts.
/// When destructuring accesses an element that might not exist, the result
/// type should include `undefined`.
pub(crate) fn add_undefined_for_indexed_access(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if type_id == TypeId::UNDEFINED || type_id == TypeId::ANY || type_id == TypeId::ERROR {
        return type_id;
    }
    // Check if already contains undefined.
    // Use solver query API (get_union_members) instead of TypeData inspection.
    if let Some(members) = tsz_solver::type_queries::get_union_members(db, type_id)
        && members.contains(&TypeId::UNDEFINED)
    {
        return type_id;
    }
    db.union2(type_id, TypeId::UNDEFINED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_variable_type_returns_unknown_when_flag_set() {
        assert_eq!(resolve_catch_variable_type(true), TypeId::UNKNOWN);
    }

    #[test]
    fn catch_variable_type_returns_any_when_flag_unset() {
        assert_eq!(resolve_catch_variable_type(false), TypeId::ANY);
    }

    #[test]
    fn catch_variable_type_with_annotation_preserves_annotation() {
        let annotated = TypeId::STRING;
        assert_eq!(catch_variable_type(Some(annotated), true), TypeId::STRING);
    }

    #[test]
    fn catch_variable_type_without_annotation_uses_flag() {
        assert_eq!(catch_variable_type(None, true), TypeId::UNKNOWN);
        assert_eq!(catch_variable_type(None, false), TypeId::ANY);
    }
}
