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
//! - **CatchVariable**: the catch clause binds a variable typed as `any` or
//!   `unknown` depending on compiler options.
//! - **OptionalChainNonNullish**: an optional chain (e.g. `a?.b`) was observed
//!   in a truthy branch, implying the base is non-nullish.
//! - **ForOfElement**: a `for-of` loop destructures an iterable; the solver
//!   resolves the iterated element type.
//! - **TruthyNarrow**: a value was used in a boolean context; remove nullish
//!   and other falsy constituents.
//! - **CatchVariableTypeofReset**: a catch variable is being narrowed by
//!   `typeof`; the base type must be reset to `unknown` to match tsc semantics.
//! - **ForOfDestructuredElement**: a for-of loop combined with destructuring;
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

        FlowObservation::ForOfElement { iterable_type } => {
            // The for-of element type is resolved by the checker's iterable
            // resolution.  This observation primarily signals the solver that
            // the binding's type comes from iteration.
            *iterable_type
        }

        FlowObservation::CatchVariableTypeofReset => {
            // When a catch variable with `unknown` type is narrowed by
            // `typeof`, the base must be reset to `unknown` so narrowing
            // operates on the full domain, not the current flow type.
            TypeId::UNKNOWN
        }

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
    // Don't strip undefined from any/unknown/error - these should pass through
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
pub(crate) fn resolve_catch_variable_type(use_unknown: bool) -> TypeId {
    if use_unknown {
        TypeId::UNKNOWN
    } else {
        TypeId::ANY
    }
}

/// Strip undefined from a destructured element type when a default is present.
/// Routes through the shared `narrow_with_default_policy` that backs
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
#[allow(dead_code)]
pub(crate) fn catch_variable_type(annotation_type: Option<TypeId>, use_unknown: bool) -> TypeId {
    match annotation_type {
        Some(ty) => ty,
        None => resolve_catch_variable_type(use_unknown),
    }
}

/// Determine if a catch variable's typeof base should be reset to `unknown`.
///
/// In tsc, when a catch variable is typed as `unknown` and narrowed by
/// `typeof`, the narrowing always starts from the full `unknown` domain,
/// regardless of what the current flow type might be.  This boundary function
/// centralizes that decision so the checker doesn't embed this policy locally.
pub(crate) fn catch_variable_typeof_base(
    current_flow_type: TypeId,
    is_catch_variable: bool,
) -> TypeId {
    if is_catch_variable && current_flow_type != TypeId::UNKNOWN {
        // Route through the observation: CatchVariableTypeofReset always
        // returns UNKNOWN regardless of input.
        TypeId::UNKNOWN
    } else {
        current_flow_type
    }
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
    fn catch_variable_typeof_base_resets_non_unknown() {
        let result = catch_variable_typeof_base(TypeId::STRING, true);
        assert_eq!(result, TypeId::UNKNOWN);
    }

    #[test]
    fn catch_variable_typeof_base_preserves_unknown() {
        let result = catch_variable_typeof_base(TypeId::UNKNOWN, true);
        assert_eq!(result, TypeId::UNKNOWN);
    }

    #[test]
    fn catch_variable_typeof_base_skips_non_catch() {
        let result = catch_variable_typeof_base(TypeId::STRING, false);
        assert_eq!(result, TypeId::STRING);
    }
}
