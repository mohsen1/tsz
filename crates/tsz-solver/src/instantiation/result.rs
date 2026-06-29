//! Typed result of a generic type instantiation.
//!
//! Every instantiator entry point used to repeat the same
//! `if instantiator.depth_exceeded { TypeId::ERROR } else { result }` collapse
//! after calling `TypeInstantiator::instantiate`. Centralizing that into a
//! typed [`InstantiationResult`] lets the engine return both pieces of
//! information explicitly while the wrapper APIs (`instantiate_type`,
//! `substitute_this_type`, ...) keep returning a plain `TypeId`.
//!
//! See [`super::request::InstantiationRequest`] for the matching request
//! boundary.

use crate::types::TypeId;

/// Whether an instantiation walk ran to completion or hit its depth guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstantiationTermination {
    /// The walk finished within the instantiation recursion-depth budget.
    Complete,
    /// The instantiation recursion-depth guard cut the walk short.
    DepthExceeded,
}

impl InstantiationTermination {
    /// Convert the raw per-walk guard bit into a named termination verdict at
    /// the result boundary.
    pub const fn from_depth_exceeded(depth_exceeded: bool) -> Self {
        if depth_exceeded {
            Self::DepthExceeded
        } else {
            Self::Complete
        }
    }

    pub const fn depth_exceeded(self) -> bool {
        matches!(self, Self::DepthExceeded)
    }
}

/// The outcome of one instantiation walk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstantiationResult {
    type_id: TypeId,
    termination: InstantiationTermination,
}

/// Instantiation result plus the verdict for project-wide cache publication.
///
/// #14346 is moving cache eligibility away from loose local booleans and into
/// typed result boundaries. The project-wide instantiation cache needs the
/// walk's [`InstantiationResult`] and whether surrounding sticky state stayed
/// clean for this request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InstantiationMemoResult {
    result: InstantiationResult,
    cache_stability: InstantiationMemoStability,
}

/// Whether an instantiation result is safe for the project-wide cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstantiationMemoStability {
    /// The walk completed and surrounding request state stayed cache-stable.
    Stable,
    /// The walk or surrounding request state was budget-limited/tainted.
    Unstable,
}

impl InstantiationMemoStability {
    const fn from_result(result: InstantiationResult, request_state_stable: bool) -> Self {
        if result.depth_exceeded() || !request_state_stable {
            Self::Unstable
        } else {
            Self::Stable
        }
    }

    const fn is_stable_for_project_cache(self) -> bool {
        matches!(self, Self::Stable)
    }
}

impl InstantiationResult {
    /// Construct a successful result.
    pub const fn ok(type_id: TypeId) -> Self {
        Self {
            type_id,
            termination: InstantiationTermination::Complete,
        }
    }

    /// Construct a result that hit the recursion-depth guard with no usable
    /// partial type. Reserved for callers that have nothing better than the
    /// `TypeId::ERROR` sentinel to report; prefer [`Self::overflow_with`].
    pub const fn overflow() -> Self {
        Self::overflow_with(TypeId::ERROR)
    }

    /// Construct a result that hit the recursion-depth guard but carries a
    /// relation-preserving partial type from the walk.
    ///
    /// The depth/frame guard now bails through
    /// `TypeInstantiator::bail_value`, which never surfaces a
    /// substitution-bound type parameter, so the partial `type_id` is a safe
    /// (deferred/opaque) approximation rather than a leak. We keep it instead
    /// of collapsing to `TypeId::ERROR` so a downstream consumer (e.g.
    /// iterator-element resolution on a fully-concrete `Map<K, V>`) does not
    /// fall back to the original un-instantiated declaration and resurface a
    /// free `T` into a concrete context (#13652). The termination verdict is
    /// still set so the cross-call cache refuses to memoize a budget-limited
    /// result.
    pub const fn overflow_with(type_id: TypeId) -> Self {
        Self {
            type_id,
            termination: InstantiationTermination::DepthExceeded,
        }
    }

    /// Construct from the walk's partial type and named termination verdict.
    pub const fn from_walk(type_id: TypeId, termination: InstantiationTermination) -> Self {
        if termination.depth_exceeded() {
            Self::overflow_with(type_id)
        } else {
            Self::ok(type_id)
        }
    }

    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    pub const fn depth_exceeded(self) -> bool {
        self.termination.depth_exceeded()
    }

    pub const fn termination(self) -> InstantiationTermination {
        self.termination
    }

    /// Collapse the result to a single `TypeId`.
    ///
    /// On overflow this now returns the relation-preserving partial type from
    /// the walk (see [`Self::overflow_with`]) rather than `TypeId::ERROR`, so
    /// consumers that lack a depth-aware path still receive a leak-free
    /// approximation instead of a sentinel that triggers an un-instantiated
    /// fallback.
    pub const fn into_type_id(self) -> TypeId {
        self.type_id
    }
}

impl InstantiationMemoResult {
    /// Construct a memo result from a typed instantiation result and the
    /// request-state stability verdict.
    pub(crate) const fn for_project_cache(
        result: InstantiationResult,
        request_state_stable: bool,
    ) -> Self {
        Self {
            result,
            cache_stability: InstantiationMemoStability::from_result(result, request_state_stable),
        }
    }

    /// Whether this result can be stored in the project-wide instantiation
    /// cache, whose key does not capture ambient recursion/fuel state.
    pub(crate) const fn is_stable_for_project_cache(self) -> bool {
        self.cache_stability.is_stable_for_project_cache()
    }

    /// Collapse to the request's instantiation result while preserving today's
    /// behavior for callers that are not yet cache-verdict-aware.
    pub(crate) const fn into_result(self) -> InstantiationResult {
        self.result
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InstantiationMemoResult, InstantiationMemoStability, InstantiationResult,
        InstantiationTermination,
    };
    use crate::types::TypeId;

    #[test]
    fn ok_result_passes_through_type_id() {
        let r = InstantiationResult::ok(TypeId::NUMBER);
        assert_eq!(r.type_id(), TypeId::NUMBER);
        assert!(!r.depth_exceeded());
        assert_eq!(r.termination(), InstantiationTermination::Complete);
        assert_eq!(r.into_type_id(), TypeId::NUMBER);
    }

    #[test]
    fn overflow_result_reports_sentinel_when_no_partial() {
        let r = InstantiationResult::overflow();
        assert!(r.depth_exceeded());
        assert_eq!(r.termination(), InstantiationTermination::DepthExceeded);
        // `overflow()` (no partial) still surfaces the `ERROR` sentinel.
        assert_eq!(r.into_type_id(), TypeId::ERROR);
    }

    #[test]
    fn overflow_with_keeps_partial_type() {
        // A depth/frame bail carries its relation-preserving partial type
        // (never a substitution-bound free param) instead of collapsing to
        // `ERROR`, so consumers do not fall back to an un-instantiated
        // original and resurface a free `T` (#13652).
        let r = InstantiationResult::overflow_with(TypeId::STRING);
        assert!(r.depth_exceeded());
        assert_eq!(r.termination(), InstantiationTermination::DepthExceeded);
        assert_eq!(r.into_type_id(), TypeId::STRING);
    }

    #[test]
    fn from_walk_routes_depth_flag() {
        let ok = InstantiationResult::from_walk(TypeId::STRING, InstantiationTermination::Complete);
        assert_eq!(ok.into_type_id(), TypeId::STRING);
        assert_eq!(ok.termination(), InstantiationTermination::Complete);

        // A depth-exceeded walk keeps the partial type the instantiator
        // produced (the relation-preserving bail value) while still flagging
        // the overflow so the cross-call cache refuses to memoize it.
        let bad =
            InstantiationResult::from_walk(TypeId::STRING, InstantiationTermination::DepthExceeded);
        assert!(bad.depth_exceeded());
        assert_eq!(bad.termination(), InstantiationTermination::DepthExceeded);
        assert_eq!(bad.into_type_id(), TypeId::STRING);
    }

    #[test]
    fn termination_names_depth_guard_bit() {
        assert_eq!(
            InstantiationTermination::from_depth_exceeded(false),
            InstantiationTermination::Complete
        );
        assert_eq!(
            InstantiationTermination::from_depth_exceeded(true),
            InstantiationTermination::DepthExceeded
        );
    }

    #[test]
    fn memo_result_requires_clean_instantiation_and_request_state() {
        let stable = InstantiationMemoResult::for_project_cache(
            InstantiationResult::ok(TypeId::STRING),
            true,
        );
        assert_eq!(stable.cache_stability, InstantiationMemoStability::Stable);
        assert!(stable.is_stable_for_project_cache());
        assert_eq!(stable.into_result().type_id(), TypeId::STRING);

        let request_state_tainted = InstantiationMemoResult::for_project_cache(
            InstantiationResult::ok(TypeId::NUMBER),
            false,
        );
        assert_eq!(
            request_state_tainted.cache_stability,
            InstantiationMemoStability::Unstable
        );
        assert!(!request_state_tainted.is_stable_for_project_cache());
        assert_eq!(
            request_state_tainted.into_result().type_id(),
            TypeId::NUMBER
        );

        let overflowed = InstantiationMemoResult::for_project_cache(
            InstantiationResult::overflow_with(TypeId::BOOLEAN),
            true,
        );
        assert_eq!(
            overflowed.cache_stability,
            InstantiationMemoStability::Unstable
        );
        assert!(!overflowed.is_stable_for_project_cache());
        assert_eq!(overflowed.into_result().type_id(), TypeId::BOOLEAN);
    }
}
