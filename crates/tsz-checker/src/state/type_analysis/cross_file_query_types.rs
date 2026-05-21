//! Typed cross-file query API types.
//!
//! This module owns the three types that form the typed cross-file query
//! contract:
//!
//! - [`CrossFileQueryKind`]: typed bucket discriminant for cache lookups and
//!   writes. The **`u8` discriminant values** are the stable contract; variant
//!   names may be renamed freely.
//! - [`CrossFileQueryKey`]: typed cache key for cross-file query lookups.
//! - [`CrossFileQueryAnswer`]: typed answer payload returned by typed
//!   query paths.
//!
//! `CrossFileQueryKey` and `CrossFileQueryAnswer` are `pub(crate)` and
//! currently unused. They exist so subsequent PR 6B+ migrations can reference
//! them from day one without introducing the type alongside the migration.
//!
use crate::context::RequestCacheKey;
use tsz_binder::SymbolId;

/// Typed identifier for the cross-file query bucket a cache lookup or write
/// targets. Replaces the four `u8` constants that used to live inline at each
/// call site.
///
/// **Stable contract: the `u8` discriminant values, not the variant names.**
/// The storage layer keys caches by `(u8, file_idx, primary, secondary,
/// args_hash)`. Renaming a variant is safe; changing its discriminant value
/// silently invalidates every `DefinitionStore` entry under that bucket.
///
/// Current buckets and their stable discriminants:
///
/// | Variant                  | `u8` |
/// |--------------------------|------|
/// | `Symbol`                 |  4   |
/// | `ClassInstance`          |  2   |
/// | `Interface`              |  1   |
/// | `InterfaceMemberSimple`  |  3   |
///
/// Adding a new bucket: add the variant with a fresh discriminant that does
/// not collide with the values above.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub(crate) enum CrossFileQueryKind {
    Interface = 1,
    ClassInstance = 2,
    InterfaceMemberSimple = 3,
    Symbol = 4,
}

impl CrossFileQueryKind {
    /// Discriminant value used as the first component of
    /// `DefinitionStore::resolved_cross_file_queries` cache keys. Stable -
    /// changing this for an existing variant would invalidate every cached
    /// entry under that discriminant.
    #[inline]
    pub(crate) const fn as_storage_kind(self) -> u8 {
        self as u8
    }
}

/// Typed cache key for cross-file query lookups.
///
/// Every input that changes the answer must appear in the key. The storage
/// layer keys caches by `(u8, file_idx, primary, secondary, args_hash)`, so
/// this struct projects the same dimensions onto the typed API:
///
/// - `kind` becomes the storage `u8` via `CrossFileQueryKind::as_storage_kind`.
/// - `target_file_idx` is the storage `file_idx`.
/// - `symbol_id.0` is the storage `primary`.
/// - `request_key` and `options_fingerprint` together feed the storage
///   `secondary` + `args_hash` slots; the projection rule is finalized
///   when the first PR 6B+ migration ships.
///
/// **Currently unused.** This struct exists so subsequent typed-query PRs
/// can reference it from day one without introducing the type alongside
/// the migration.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct CrossFileQueryKey {
    pub kind: CrossFileQueryKind,
    pub target_file_idx: u32,
    pub symbol_id: SymbolId,
    pub request_key: Option<RequestCacheKey>,
    pub options_fingerprint: u64,
}

/// Typed answer payload for cross-file query results.
///
/// Variants:
///
/// - `Type`: a single `TypeId` answer (e.g. interface-member type lookup).
/// - `TypeWithParams`: a `TypeId` plus the type-parameter info needed to
///   instantiate it (e.g. symbol-type lookup for a generic alias).
/// - `MemberType`: a member-name → type pair (e.g. namespace member or
///   interface property).
/// - `Unknown`: the target file did not produce a typed answer; the caller
///   should fall back to child-checker construction.
/// - `Error`: the typed query path itself failed (e.g. recursion limit,
///   inaccessible symbol). Distinct from `Unknown` so callers can avoid
///   re-entering the slow path.
///
/// **Currently unused.** Same shipping rationale as `CrossFileQueryKey`.
#[derive(Clone, Debug)]
pub(crate) enum CrossFileQueryAnswer {
    Type(tsz_solver::TypeId),
    TypeWithParams(tsz_solver::TypeId, Vec<tsz_solver::TypeParamInfo>),
    MemberType {
        member: tsz_common::interner::Atom,
        ty: tsz_solver::TypeId,
    },
    Unknown,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CrossFileQueryKey` should be `Hash + PartialEq + Eq + Clone` so it
    /// can be used as a cache map key. Compile-time check via `_test()`
    /// keeps the contract enforced even if a future PR removes a derive.
    #[test]
    fn key_implements_required_traits() {
        fn _test<T: Clone + std::fmt::Debug + std::hash::Hash + Eq>() {}
        _test::<CrossFileQueryKey>();
    }

    /// Two keys with identical fields must hash and compare equal so
    /// `HashMap<CrossFileQueryKey, _>` lookups round-trip.
    #[test]
    fn key_hash_and_eq_round_trip() {
        let key = CrossFileQueryKey {
            kind: CrossFileQueryKind::Symbol,
            target_file_idx: 7,
            symbol_id: SymbolId(42),
            request_key: None,
            options_fingerprint: 0xDEAD_BEEF,
        };
        let same = key.clone();
        assert_eq!(key, same);
        let mut map: std::collections::HashMap<CrossFileQueryKey, u32> =
            std::collections::HashMap::new();
        map.insert(key, 1);
        assert_eq!(map.get(&same), Some(&1));
    }

    /// All five answer variants should be constructible. Smoke test that
    /// catches accidental variant removal during refactors.
    #[test]
    fn answer_variants_constructible() {
        let _t: CrossFileQueryAnswer = CrossFileQueryAnswer::Type(tsz_solver::TypeId::ANY);
        let _tp: CrossFileQueryAnswer = CrossFileQueryAnswer::TypeWithParams(
            tsz_solver::TypeId::ANY,
            Vec::<tsz_solver::TypeParamInfo>::new(),
        );
        let _m: CrossFileQueryAnswer = CrossFileQueryAnswer::MemberType {
            member: tsz_common::interner::Atom::default(),
            ty: tsz_solver::TypeId::ANY,
        };
        let _u: CrossFileQueryAnswer = CrossFileQueryAnswer::Unknown;
        let _e: CrossFileQueryAnswer = CrossFileQueryAnswer::Error;
    }
}
