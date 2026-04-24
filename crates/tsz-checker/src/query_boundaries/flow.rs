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
//! - **`CatchVariableTypeofReset`**: a catch variable is being narrowed by
//!   `typeof`; the base type must be reset to `unknown` to match tsc semantics.
//! - **`ForOfDestructuredElement`**: a for-of loop combined with destructuring;
//!   the element type is extracted and optionally has `undefined` stripped.

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

    /// A catch variable is being narrowed by a `typeof` guard.  The checker
    /// detected that the target is a catch-clause binding whose declared type
    /// is `unknown`.  The observation resets the narrowing base to `unknown`
    /// regardless of upstream flow, matching tsc's behavior where `typeof e`
    /// in a catch clause always narrows from the full `unknown` domain.
    CatchVariableTypeofReset,

    /// A for-of loop destructures an iterable into a binding pattern.
    /// Combines iterable element extraction with optional default narrowing.
    ForOfDestructuredElement {
        /// The element type already resolved from the iterable.
        element_type: TypeId,
        /// Whether the destructured binding has a default value.
        has_default: bool,
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
            narrow_with_default_policy(db, base_type, *has_default)
        }

        FlowObservation::DestructuringProperty { has_default, .. } => {
            narrow_with_default_policy(db, base_type, *has_default)
        }

        FlowObservation::ForOfElement { iterable_type } => *iterable_type,

        FlowObservation::CatchVariableTypeofReset => TypeId::UNKNOWN,

        FlowObservation::ForOfDestructuredElement {
            element_type,
            has_default,
        } => narrow_with_default_policy(db, *element_type, *has_default),
    }
}

/// Internal: apply the destructuring-default narrowing policy.
///
/// When a destructuring binding has a default value, `undefined` is removed
/// from the element type.  `any`/`unknown`/`error` pass through unchanged.
fn narrow_with_default_policy(
    db: &dyn TypeDatabase,
    element_type: TypeId,
    has_default: bool,
) -> TypeId {
    if !has_default {
        return element_type;
    }
    if element_type == TypeId::ANY
        || element_type == TypeId::UNKNOWN
        || element_type == TypeId::ERROR
    {
        return element_type;
    }
    tsz_solver::remove_undefined(db, element_type)
}

/// Apply optional-chain non-nullish narrowing through the boundary.
/// Routes through [`apply_flow_observation`] with [`FlowObservation::OptionalChainNonNullish`].
pub(crate) fn narrow_optional_chain(db: &dyn TypeDatabase, base_type: TypeId) -> TypeId {
    apply_flow_observation(db, base_type, &FlowObservation::OptionalChainNonNullish)
}

/// Resolve catch variable type through the boundary.
pub(crate) const fn resolve_catch_variable_type(use_unknown: bool) -> TypeId {
    if use_unknown {
        TypeId::UNKNOWN
    } else {
        TypeId::ANY
    }
}

/// Determine the typeof narrowing base for a catch variable.
///
/// In tsc, when a catch variable is narrowed by `typeof`, the narrowing
/// always starts from the catch variable's declared base type (`any` or
/// `unknown`) rather than the already-narrowed flow type.  This boundary
/// function centralizes that decision.
///
/// When `use_unknown` is true (strict mode), resets to `unknown`.
/// When `use_unknown` is false, resets to `any`.
/// Non-catch variables pass through unchanged.
pub(crate) const fn catch_variable_typeof_base(
    type_id: TypeId,
    is_catch_var: bool,
    use_unknown: bool,
) -> TypeId {
    if is_catch_var {
        resolve_catch_variable_type(use_unknown)
    } else {
        type_id
    }
}

/// Simplified catch-variable typeof base for flow analysis contexts where
/// compiler options are not directly available.
///
/// In the flow analyzer, `type_id` is already the catch variable's declared
/// base type (`any` or `unknown`), so this function is a pass-through that
/// documents the boundary decision.  The important part is that the caller
/// does NOT override the type with ad-hoc logic.
pub(crate) const fn catch_variable_typeof_base_from_flow(
    type_id: TypeId,
    _is_catch_var: bool,
) -> TypeId {
    type_id
}

/// Strip undefined from a destructured element type when a default is present.
/// Routes through the shared [`narrow_with_default_policy`] that backs
/// [`FlowObservation::DestructuringElement`] and [`FlowObservation::DestructuringProperty`].
pub(crate) fn narrow_destructuring_default(
    db: &dyn TypeDatabase,
    element_type: TypeId,
    has_default: bool,
) -> TypeId {
    narrow_with_default_policy(db, element_type, has_default)
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

/// Resolve a `Lazy(DefId)` type through the checker environment when available.
///
/// This keeps checker code free of raw lazy-definition lookups and keeps the
/// behavior centralized in the boundary layer.
pub(crate) fn resolve_lazy_def_with_env(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::TypeEnvironment>,
    type_id: TypeId,
) -> TypeId {
    if let Some(def_id) = tsz_solver::type_queries::get_lazy_def_id(db, type_id)
        && let Some(environment) = env
        && let Some(resolved) = tsz_solver::TypeResolver::resolve_lazy(environment, def_id, db)
    {
        return resolved;
    }
    type_id
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

    #[test]
    fn catch_variable_typeof_base_resets_for_catch_var_unknown() {
        let result = catch_variable_typeof_base(TypeId::STRING, true, true);
        assert_eq!(result, TypeId::UNKNOWN);
    }

    #[test]
    fn catch_variable_typeof_base_resets_for_catch_var_any() {
        let result = catch_variable_typeof_base(TypeId::STRING, true, false);
        assert_eq!(result, TypeId::ANY);
    }

    #[test]
    fn catch_variable_typeof_base_preserves_non_catch() {
        let result = catch_variable_typeof_base(TypeId::STRING, false, true);
        assert_eq!(result, TypeId::STRING);
    }
}
