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

    /// Construct a result that hit the recursion-depth guard. Callers should
    /// flatten this to [`TypeId::ERROR`] via [`Self::into_type_id`] when they
    /// need a single `TypeId` to hand to legacy code paths.
    pub const fn overflow() -> Self {
        Self {
            type_id: TypeId::ERROR,
            overflowed: true,
        }
    }

    /// Construct from a `(type_id, depth_exceeded)` pair as produced by the
    /// raw instantiator walk.
    pub const fn from_walk(type_id: TypeId, depth_exceeded: bool) -> Self {
        if depth_exceeded {
            Self::overflow()
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

    /// Collapse the result to a single `TypeId`, replacing depth-exceeded
    /// failures with `TypeId::ERROR`. This is the conversion every legacy
    /// `_cached` entry performed inline before this refactor.
    pub const fn into_type_id(self) -> TypeId {
        if self.overflowed {
            TypeId::ERROR
        } else {
            self.type_id
        }
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
    fn overflow_result_collapses_to_error() {
        let r = InstantiationResult::overflow();
        assert!(r.depth_exceeded());
        assert_eq!(r.into_type_id(), TypeId::ERROR);
    }

    #[test]
    fn from_walk_routes_depth_flag() {
        let ok = InstantiationResult::from_walk(TypeId::STRING, false);
        assert_eq!(ok.into_type_id(), TypeId::STRING);

        // A depth-exceeded walk discards whatever partial type the
        // instantiator produced and reports `ERROR`.
        let bad = InstantiationResult::from_walk(TypeId::STRING, true);
        assert!(bad.depth_exceeded());
        assert_eq!(bad.into_type_id(), TypeId::ERROR);
    }
}
