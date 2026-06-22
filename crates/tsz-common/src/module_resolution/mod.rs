//! Shared module-resolution primitives used across tsz crates.
//!
//! These are pure, dependency-light helpers (string + `serde_json` only) so
//! both the CLI driver resolver and the checker's resolution boundary can
//! share one implementation instead of maintaining divergent copies.

pub mod package_exports;
pub mod path_identity;
pub mod types_versions;

/// Three-way outcome of resolving a single `package.json` `exports`/`imports`
/// target, mirroring Node.js `PACKAGE_TARGET_RESOLVE` (which `tsc`
/// reimplements in `moduleNameResolver`).
///
/// The Node algorithm distinguishes a deliberately *blocked* mapping from a
/// mere *miss*, and the two have opposite control flow. Representing both as
/// `Option::None` (a miss) is the defect this type fixes: a JSON `null` reached
/// through a matching condition must stop the whole search, not fall through to
/// a sibling.
///
/// - [`Resolved`](TargetMatch::Resolved): a usable target was produced.
/// - [`Blocked`](TargetMatch::Blocked): an explicit JSON `null` was reached
///   through a *matching* condition, array element, or exact subpath key. Per
///   the spec this terminates the entire `exports`/`imports` resolution — it
///   must NOT fall through to a sibling condition, a later array element, the
///   enclosing conditional, or pattern matching. Both Node and `tsc` then
///   report the specifier as not exported (`TS2307`).
/// - [`NotApplicable`](TargetMatch::NotApplicable): no usable target here (the
///   condition did not match, or the resolved file is missing). The caller
///   keeps searching siblings/fallbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetMatch<T> {
    /// A usable target was produced.
    Resolved(T),
    /// An explicit JSON `null` blocked the whole resolution.
    Blocked,
    /// No usable target here; keep searching.
    NotApplicable,
}

impl<T> TargetMatch<T> {
    /// `true` only for [`TargetMatch::Blocked`].
    pub const fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked)
    }

    /// Map the payload of a [`TargetMatch::Resolved`], leaving the
    /// `Blocked`/`NotApplicable` control states untouched.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> TargetMatch<U> {
        use TargetMatch::{Blocked, NotApplicable, Resolved};

        match self {
            Resolved(value) => Resolved(f(value)),
            Blocked => Blocked,
            NotApplicable => NotApplicable,
        }
    }

    /// Collapse to an [`Option`], discarding the block/miss distinction. Only
    /// safe at a boundary where a block and a miss lead to the same outcome
    /// (both mean "this layer produced nothing").
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Resolved(value) => Some(value),
            Self::Blocked | Self::NotApplicable => None,
        }
    }
}

impl<T> TargetMatch<Vec<T>> {
    /// Wrap an accumulated candidate list: a non-empty list is
    /// [`TargetMatch::Resolved`]; an empty list is [`TargetMatch::NotApplicable`]
    /// ("no matching condition produced a target — keep searching"). A `null`
    /// block is reported separately by the caller and is never collapsed here.
    pub fn from_candidates(candidates: Vec<T>) -> Self {
        if candidates.is_empty() {
            Self::NotApplicable
        } else {
            Self::Resolved(candidates)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TargetMatch;

    #[test]
    fn map_only_transforms_the_resolved_payload() {
        assert_eq!(
            TargetMatch::Resolved(2).map(|n| n * 3),
            TargetMatch::Resolved(6)
        );
        assert_eq!(
            TargetMatch::<i32>::Blocked.map(|n| n * 3),
            TargetMatch::Blocked
        );
        assert_eq!(
            TargetMatch::<i32>::NotApplicable.map(|n| n * 3),
            TargetMatch::NotApplicable
        );
    }

    #[test]
    fn is_blocked_and_into_option_distinguish_block_from_miss() {
        assert!(TargetMatch::<i32>::Blocked.is_blocked());
        assert!(!TargetMatch::Resolved(1).is_blocked());
        assert!(!TargetMatch::<i32>::NotApplicable.is_blocked());

        // `into_option` deliberately collapses the block/miss distinction.
        assert_eq!(TargetMatch::Resolved(7).into_option(), Some(7));
        assert_eq!(TargetMatch::<i32>::Blocked.into_option(), None);
        assert_eq!(TargetMatch::<i32>::NotApplicable.into_option(), None);
    }
}
