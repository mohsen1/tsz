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

/// The outcome of one instantiation walk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstantiationResult {
    type_id: TypeId,
    overflowed: bool,
}

impl InstantiationResult {
    /// Construct a successful result.
    pub const fn ok(type_id: TypeId) -> Self {
        Self {
            type_id,
            overflowed: false,
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
    /// free `T` into a concrete context (#13652). The `overflowed` flag is
    /// still set so the cross-call cache refuses to memoize a budget-limited
    /// result.
    pub const fn overflow_with(type_id: TypeId) -> Self {
        Self {
            type_id,
            overflowed: true,
        }
    }

    /// Construct from a `(type_id, depth_exceeded)` pair as produced by the
    /// raw instantiator walk.
    pub const fn from_walk(type_id: TypeId, depth_exceeded: bool) -> Self {
        if depth_exceeded {
            Self::overflow_with(type_id)
        } else {
            Self::ok(type_id)
        }
    }

    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    pub const fn depth_exceeded(self) -> bool {
        self.overflowed
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

#[cfg(test)]
mod tests {
    use super::InstantiationResult;
    use crate::types::TypeId;

    #[test]
    fn ok_result_passes_through_type_id() {
        let r = InstantiationResult::ok(TypeId::NUMBER);
        assert_eq!(r.type_id(), TypeId::NUMBER);
        assert!(!r.depth_exceeded());
        assert_eq!(r.into_type_id(), TypeId::NUMBER);
    }

    #[test]
    fn overflow_result_reports_sentinel_when_no_partial() {
        let r = InstantiationResult::overflow();
        assert!(r.depth_exceeded());
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
        assert_eq!(r.into_type_id(), TypeId::STRING);
    }

    #[test]
    fn from_walk_routes_depth_flag() {
        let ok = InstantiationResult::from_walk(TypeId::STRING, false);
        assert_eq!(ok.into_type_id(), TypeId::STRING);

        // A depth-exceeded walk keeps the partial type the instantiator
        // produced (the relation-preserving bail value) while still flagging
        // the overflow so the cross-call cache refuses to memoize it.
        let bad = InstantiationResult::from_walk(TypeId::STRING, true);
        assert!(bad.depth_exceeded());
        assert_eq!(bad.into_type_id(), TypeId::STRING);
    }
}
